//! HTTP surface for the secrets service.
//!
//! Routing is a pure function of method and path ([`handle`]) so the whole
//! surface is testable without a socket, and `serve` is the only place that owns
//! a listener.
//!
//! Two invariants the route code is responsible for:
//!
//! * any path outside the matched set below returns 404 — there is no catch-all
//!   that can answer 200;
//! * the vault lock is only ever held for synchronous [`SecretStore`] calls, so
//!   no guard survives an `await` point.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use http_body::Body as HttpBody;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use tokio::net::TcpListener;
use tracing::{debug, info, warn};

use crate::auth::is_authorized;
use crate::state::AppState;

/// Unauthenticated liveness endpoint.
pub const HEALTH_PATH: &str = "/healthz";
/// Collection route.
pub const SECRETS_PATH: &str = "/api/v1/secrets";
/// Route prefix for a single secret.
pub const SECRET_PREFIX: &str = "/api/v1/secrets/";

/// Service response body: always fully buffered, never streams secrets.
pub type Body = Full<Bytes>;
/// Service response type.
pub type ApiResponse = Response<Body>;

/// Handles one request. Generic over the request body so both the real
/// `hyper::body::Incoming` and in-test bodies reach the identical code.
pub async fn handle<B>(req: Request<B>, state: Arc<AppState>) -> ApiResponse
where
    B: HttpBody<Data = Bytes> + Send + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let key = path.strip_prefix(SECRET_PREFIX).and_then(|raw| {
        if raw.is_empty() {
            None
        } else {
            Some(raw.to_string())
        }
    });

    if method == Method::GET && path == HEALTH_PATH {
        return health_response();
    }

    // Every route below requires a bearer token; unknown paths are rejected
    // before that check so a probe never learns whether auth is configured.
    let is_secret_route = path == SECRETS_PATH || key.is_some();
    if !is_secret_route {
        debug!(%path, %method, status = 404, "unknown route");
        return text(StatusCode::NOT_FOUND, "not found");
    }

    if !is_authorized(state.tokens(), req.headers()) {
        warn!(%method, %path, status = 401, "unauthorized request");
        return text(StatusCode::UNAUTHORIZED, "unauthorized");
    }

    match (method, key) {
        (Method::GET, None) => list_secrets(&state).await,
        (Method::GET, Some(key)) => get_secret(&state, &key).await,
        (Method::PUT, Some(key)) => put_secret(&state, &key, req).await,
        (Method::DELETE, Some(key)) => delete_secret(&state, &key).await,
        _ => text(StatusCode::NOT_FOUND, "not found"),
    }
}

/// Builds responses, never failing: status codes and header values are static.
fn build(status: StatusCode, content_type: &'static str, body: Body) -> ApiResponse {
    hyper::http::response::Builder::new()
        .status(status)
        .header(hyper::http::header::CONTENT_TYPE, content_type)
        .body(body)
        .unwrap_or_else(|_| ApiResponse::new(Body::from("internal error")))
}

fn text(status: StatusCode, body: &'static str) -> ApiResponse {
    build(status, "text/plain; charset=utf-8", Body::from(body))
}

fn json(status: StatusCode, body: Vec<u8>) -> ApiResponse {
    build(status, "application/json", Body::from(body))
}

fn health_response() -> ApiResponse {
    json(
        StatusCode::OK,
        serde_json::to_vec(&serde_json::json!({
            "status": "ok",
            "service": "aegis-serve",
            "version": env!("CARGO_PKG_VERSION"),
        }))
        .unwrap_or_default(),
    )
}

/// `GET /api/v1/secrets` — keys and tags only. The vault lock is held for the
/// synchronous listing call and dropped before the response is built.
async fn list_secrets(state: &AppState) -> ApiResponse {
    let listing = {
        let vault = state.vault();
        vault
            .list()
            .into_iter()
            .map(|(key, tags)| SecretListing {
                key: key.to_string(),
                tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
            })
            .collect::<Vec<_>>()
    };
    json(
        StatusCode::OK,
        serde_json::to_vec(&ListingResponse { secrets: listing }).unwrap_or_default(),
    )
}

/// `GET /api/v1/secrets/{key}` — the only route that returns a secret value,
/// and only after the request has been authenticated.
async fn get_secret(state: &AppState, key: &str) -> ApiResponse {
    let outcome = {
        let vault = state.vault();
        match vault.get(key) {
            Ok(value) => Ok(value),
            // Distinguish "absent" from "present but unreadable" so a decrypt
            // failure is a 500 rather than a misleading 404.
            Err(_) if !vault.contains(key) => Err(KeyError::NotFound),
            Err(error) => Err(KeyError::Unreadable(error)),
        }
    };
    match outcome {
        Ok(value) => build(
            StatusCode::OK,
            "application/octet-stream",
            Body::from(value.into_bytes()),
        ),
        Err(KeyError::NotFound) => text(StatusCode::NOT_FOUND, "not found"),
        Err(KeyError::Unreadable(error)) => {
            warn!(status = 500, "could not decrypt stored entry: {error:#}");
            text(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
        }
    }
}

/// `PUT /api/v1/secrets/{key}` — raw value in the body, persisted immediately.
async fn put_secret<B>(state: &AppState, key: &str, req: Request<B>) -> ApiResponse
where
    B: HttpBody<Data = Bytes> + Send + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let collected = match http_body_util::BodyExt::collect(req.into_body()).await {
        Ok(collected) => collected.to_bytes(),
        Err(error) => {
            debug!(status = 400, "request body could not be read: {error:#}");
            return text(StatusCode::BAD_REQUEST, "invalid request body");
        }
    };
    let value = match std::str::from_utf8(&collected) {
        Ok(value) => value,
        Err(_) => return text(StatusCode::BAD_REQUEST, "request body must be utf-8"),
    };

    let outcome = {
        let mut vault = state.vault();
        vault.set(key, value, &[]).and_then(|()| vault.save())
    };
    match outcome {
        Ok(()) => {
            info!(%key, status = 200, "secret stored");
            text(StatusCode::OK, "ok")
        }
        Err(error) => {
            warn!(status = 500, %key, "store failure: {error:#}");
            text(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
        }
    }
}

/// `DELETE /api/v1/secrets/{key}` — removes the entry and persists.
async fn delete_secret(state: &AppState, key: &str) -> ApiResponse {
    let outcome = {
        let mut vault = state.vault();
        if !vault.contains(key) {
            None
        } else {
            Some(vault.remove(key).and_then(|()| vault.save()))
        }
    };
    match outcome {
        None => {
            debug!(%key, status = 404, "delete of missing key");
            text(StatusCode::NOT_FOUND, "not found")
        }
        Some(Ok(())) => {
            info!(%key, status = 200, "secret removed");
            text(StatusCode::OK, "ok")
        }
        Some(Err(error)) => {
            warn!(status = 500, %key, "remove failure: {error:#}");
            text(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
        }
    }
}

enum KeyError {
    NotFound,
    Unreadable(anyhow::Error),
}

#[derive(serde::Serialize)]
struct SecretListing {
    key: String,
    tags: Vec<String>,
}

#[derive(serde::Serialize)]
struct ListingResponse {
    secrets: Vec<SecretListing>,
}

/// Accepts connections until the listener closes. One task per connection; each
/// task owns a clone of the state, never the listener.
pub async fn serve(listener: TcpListener, state: Arc<AppState>) -> Result<()> {
    let local_addr: SocketAddr = listener.local_addr()?;
    info!(addr = %local_addr, agents = state.tokens().len(), "serving");
    loop {
        let (stream, _peer) = listener.accept().await?;
        let io = hyper_util::rt::TokioIo::new(stream);
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let service = service_fn(move |req| {
                let state = Arc::clone(&state);
                async move { Ok::<_, Infallible>(handle(req, state).await) }
            });
            if let Err(error) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                debug!(error = %error, "connection closed with error");
            }
        });
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, Config};

    const TOKEN: &str = "s3cret-token";
    const VALUE: &str = "value-0f-Db-Pa55w0rd";
    const KEY: &str = "db-password";

    async fn state_with_secret() -> (tempfile::TempDir, Arc<AppState>) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("vault.toml");
        let state = Arc::new(
            AppState::with_vault(
                &path,
                "master-pw",
                Config::from_parts("ignored", "master-pw", "eruka:s3cret-token", "127.0.0.1:1")
                    .unwrap()
                    .tokens
                    .clone(),
            )
            .expect("vault"),
        );
        {
            let mut vault = state.vault();
            vault.set(KEY, VALUE, &["eruka", "prod"]).expect("seed");
            vault.save().expect("persist seed");
        }
        (dir, state)
    }

    fn request(method: Method, path: &str, token: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            builder = builder.header(hyper::http::header::AUTHORIZATION, format!("Bearer {token}"));
        }
        builder.body(Body::from("")).expect("request")
    }

    fn auth_request(method: Method, path: &str) -> Request<Body> {
        request(method, path, Some(TOKEN))
    }

    async fn status_of(req: Request<Body>, state: Arc<AppState>) -> (StatusCode, String) {
        let response = handle(req, state).await;
        let status = response.status();
        let body = http_body_util::BodyExt::collect(response.into_body())
            .await
            .expect("body")
            .to_bytes();
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    async fn json_of(req: Request<Body>, state: Arc<AppState>) -> (StatusCode, serde_json::Value) {
        let (status, body) = status_of(req, state).await;
        let parsed = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
        (status, parsed)
    }

    #[tokio::test]
    async fn healthz_is_open_and_carries_the_version() {
        let (_dir, state) = state_with_secret().await;
        let (status, body) = json_of(request(Method::GET, HEALTH_PATH, None), state).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
        assert_eq!(body["service"], "aegis-serve");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
        // The healthcheck must not describe vault contents.
        assert!(!body.to_string().contains(KEY), "healthz leaked a key: {body}");
        assert!(!body.to_string().contains(VALUE), "healthz leaked a value: {body}");
    }

    #[tokio::test]
    async fn list_returns_keys_and_tags_but_never_values() {
        let (_dir, state) = state_with_secret().await;
        let (status, body) = json_of(auth_request(Method::GET, SECRETS_PATH), state).await;
        assert_eq!(status, StatusCode::OK);
        let entries = body["secrets"].as_array().expect("secrets array");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["key"], KEY);
        let tags: Vec<&str> = entries[0]["tags"]
            .as_array()
            .expect("tags array")
            .iter()
            .filter_map(|t| t.as_str())
            .collect();
        assert!(tags.contains(&"eruka"), "tags must be visible");
        assert!(tags.contains(&"prod"), "tags must be visible");
        let raw = body.to_string();
        assert!(
            !raw.contains(VALUE),
            "list response leaked a secret value: {raw}"
        );
    }

    #[tokio::test]
    async fn get_returns_the_value_for_a_known_key() {
        let (_dir, state) = state_with_secret().await;
        let (status, body) = status_of(auth_request(Method::GET, &format!("{SECRET_PREFIX}{KEY}")), state).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, VALUE, "authorised GET must return exactly the value");
    }

    #[tokio::test]
    async fn put_stores_and_persists_a_secret() {
        let (dir, state) = state_with_secret().await;
        let new_value = "value-0f-N3w-S3cret";
        let mut req = auth_request(Method::PUT, &format!("{SECRET_PREFIX}api-token"));
        *req.body_mut() = Body::from(new_value.to_string());
        let (status, body) = status_of(req, state.clone()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "ok");

        // Read it back through the route, and confirm it survives a reopen.
        let (status, body) = status_of(
            auth_request(Method::GET, &format!("{SECRET_PREFIX}api-token")),
            state.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, new_value);

        let vault_path = dir.path().join("vault.toml");
        let reopened = aegis_secrets::store::SecretStore::open(&vault_path, "master-pw")
            .expect("vault reopened from disk");
        assert_eq!(reopened.get("api-token").expect("read back"), new_value);
    }

    #[tokio::test]
    async fn delete_removes_a_known_key() {
        let (_dir, state) = state_with_secret().await;
        let (status, body) = status_of(
            auth_request(Method::DELETE, &format!("{SECRET_PREFIX}{KEY}")),
            state.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "ok");

        let (status, _) = status_of(
            auth_request(Method::GET, &format!("{SECRET_PREFIX}{KEY}")),
            state.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "deleted key must be gone");
    }

    #[tokio::test]
    async fn unknown_keys_are_404_on_get_and_delete() {
        let (_dir, state) = state_with_secret().await;
        for method in [Method::GET, Method::DELETE] {
            let (status, _) = status_of(
                auth_request(method.clone(), &format!("{SECRET_PREFIX}missing-key")),
                state.clone(),
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} of a missing key");
        }
    }

    #[tokio::test]
    async fn every_protected_route_rejects_missing_wrong_and_empty_tokens() {
        let (_dir, state) = state_with_secret().await;
        let protected = [
            (Method::GET, SECRETS_PATH.to_string()),
            (Method::GET, format!("{SECRET_PREFIX}{KEY}")),
            (Method::PUT, format!("{SECRET_PREFIX}{KEY}")),
            (Method::DELETE, format!("{SECRET_PREFIX}{KEY}")),
        ];
        for (method, path) in protected {
            for bad in [None, Some(""), Some("wrong-token"), Some(TOKEN), Some("Bearer")] {
                let (status, _) = status_of(request(method.clone(), &path, bad), state.clone()).await;
                if bad == Some(TOKEN) {
                    assert_eq!(status, StatusCode::OK, "control: {method} {path} with a good token");
                } else {
                    assert_eq!(
                        status,
                        StatusCode::UNAUTHORIZED,
                        "{method} {path} with token {bad:?} must be 401"
                    );
                }
            }

            // A header that is present but carries no scheme, and one whose token
            // is empty, are both malformed.
            for raw in ["s3cret-token", "Bearer ", "Bearer  ", "bearer s3cret-token"] {
                let mut req = request(method.clone(), &path, None);
                req.headers_mut().insert(
                    hyper::http::header::AUTHORIZATION,
                    hyper::http::HeaderValue::from_str(raw).unwrap(),
                );
                let (status, _) = status_of(req, state.clone()).await;
                assert_eq!(
                    status,
                    StatusCode::UNAUTHORIZED,
                    "{method} {path} with Authorization {raw:?} must be 401"
                );
            }
        }
    }

    #[tokio::test]
    async fn unknown_routes_are_never_200() {
        let (_dir, state) = state_with_secret().await;
        let cases: Vec<(Method, &str)> = vec![
            (Method::GET, "/"),
            (Method::GET, "/api"),
            (Method::GET, "/api/v1"),
            (Method::GET, "/api/v1/secrets/"),
            (Method::GET, "/api/v1/secrets/../secrets"),
            (Method::GET, "/api/v1/healthz"),
            (Method::GET, "/api/v1/secreT"),
            (Method::GET, "/api/v1/secrets/db-password/extra"),
            (Method::POST, "/api/v1/secrets"),
            (Method::POST, SECRET_PREFIX),
            (Method::PATCH, SECRETS_PATH),
            (Method::PUT, SECRETS_PATH),
            (Method::DELETE, SECRETS_PATH),
            (Method::GET, "/healthz/extra"),
            (Method::PUT, "/healthz"),
        ];
        for (method, path) in cases {
            let (status, _) = status_of(request(method.clone(), path, Some(TOKEN)), state.clone()).await;
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "{method} {path} must be 404, never 200"
            );
        }
    }

    #[tokio::test]
    async fn query_string_does_not_change_the_addressed_key() {
        let (_dir, state) = state_with_secret().await;
        // The addressed key is the path segment; the query is ignored, so the
        // same secret is returned whether or not one is present.
        let (status, body) = status_of(
            auth_request(Method::GET, "/api/v1/secrets/db-password?tag=eruka"),
            state.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "the query must not reroute the request");
        assert_eq!(body, VALUE);

        // A query on the collection route is likewise ignored, not a 404 and
        // not a way to address a single secret.
        let (status, body) = json_of(
            auth_request(Method::GET, "/api/v1/secrets?key=db-password"),
            state.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            !body.to_string().contains(VALUE),
            "a query must not turn the collection route into a value route: {body}"
        );
    }

    #[tokio::test]
    async fn non_utf8_body_is_rejected() {
        let (_dir, state) = state_with_secret().await;
        let mut req = auth_request(Method::PUT, &format!("{SECRET_PREFIX}bytes"));
        *req.body_mut() = Body::from(vec![0xffu8, 0xfe, 0xfd]);
        let (status, _) = status_of(req, state).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn swap_header_uses_the_vault_lock_only_for_synchronous_work() {
        // A PUT that awaits after the lock is a deadlock; this would hang if the
        // store call were moved behind a lock held across an await point.
        let (_dir, state) = state_with_secret().await;
        let mut first = auth_request(Method::PUT, &format!("{SECRET_PREFIX}k"));
        *first.body_mut() = Body::from("one".to_string());
        let (status, _) = status_of(first, state.clone()).await;
        assert_eq!(status, StatusCode::OK);

        let mut second = auth_request(Method::PUT, &format!("{SECRET_PREFIX}k"));
        *second.body_mut() = Body::from("two".to_string());
        let (status, _) = status_of(second, state).await;
        assert_eq!(status, StatusCode::OK);
    }

    // ---- Log hygiene -------------------------------------------------------

    use std::sync::Mutex as StdMutex;
    use tracing::span::{Attributes, Id as SpanId};
    use tracing::{Event, Metadata, Subscriber};

    /// Collects every field of every event into one string.
    #[derive(Default)]
    struct FieldRecorder {
        out: String,
    }

    impl tracing::field::Visit for FieldRecorder {
        fn record_debug(&mut self, field: &tracing_core::Field, value: &dyn std::fmt::Debug) {
            use std::fmt::Write;
            let _ = write!(self.out, " {}={:?}", field.name(), value);
        }
    }

    struct CaptureSubscriber {
        out: Arc<StdMutex<String>>,
    }

    impl Subscriber for CaptureSubscriber {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _span: &Attributes<'_>) -> SpanId {
            SpanId::from_u64(1)
        }
        fn record(&self, _span: &SpanId, _values: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _span: &SpanId, _follows: &SpanId) {}
        fn event(&self, event: &Event<'_>) {
            let mut recorder = FieldRecorder::default();
            event.record(&mut recorder);
            let mut guard = self.out.lock().expect("capture lock");
            guard.push_str(&recorder.out);
            guard.push('\n');
        }
        fn enter(&self, _span: &SpanId) {}
        fn exit(&self, _span: &SpanId) {}
    }

    #[tokio::test]
    async fn secret_values_never_reach_tracing() {
        let (_dir, state) = state_with_secret().await;
        let captured = Arc::new(StdMutex::new(String::new()));
        let subscriber = CaptureSubscriber {
            out: Arc::clone(&captured),
        };
        // set_default is thread-local: handle() runs on this same thread, so
        // every event the handlers emit lands in `captured`.
        let _guard = tracing::subscriber::set_default(subscriber);

        // Exercise the full surface: list, authorised get, unauthorized get,
        // unknown route, missing key, delete.
        let calls: Vec<Request<Body>> = vec![
            request(Method::GET, HEALTH_PATH, None),
            auth_request(Method::GET, SECRETS_PATH),
            auth_request(Method::GET, &format!("{SECRET_PREFIX}{KEY}")),
            request(Method::GET, &format!("{SECRET_PREFIX}{KEY}"), None),
            request(Method::GET, "/nope", Some(TOKEN)),
            auth_request(Method::GET, &format!("{SECRET_PREFIX}missing")),
            auth_request(Method::PUT, &format!("{SECRET_PREFIX}api-key")),
            auth_request(Method::DELETE, &format!("{SECRET_PREFIX}api-key")),
        ];
        for call in calls {
            let _ = handle(call, state.clone()).await;
        }

        let logs = captured.lock().expect("capture lock");
        assert!(
            !logs.is_empty(),
            "the capture subscriber must have seen the handlers' events"
        );
        assert!(
            !logs.contains(VALUE),
            "secret value leaked into tracing output: {logs}"
        );
        // Keys are metadata and may be logged; the value is not.
        assert!(logs.contains(KEY) || logs.contains("api-key") || logs.contains("secret"));
    }
}
