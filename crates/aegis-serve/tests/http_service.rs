//! End-to-end tests: the real binary's fail-closed startup, and one in-process
//! hyper server bound to an ephemeral port exercised over real TCP.

use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use aegis_serve::routes::serve;
use aegis_serve::state::{AppState, Config};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const TOKEN: &str = "tok-eruka";
const VALUE: &str = "value-0f-Api-K3y";

/// One HTTP/1.1 round trip over a fresh connection. Returns `(status, body)`.
async fn round_trip(
    addr: std::net::SocketAddr,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&str>,
) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let mut request = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    if let Some(token) = token {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if let Some(body) = body {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("\r\n");
    if let Some(body) = body {
        request.push_str(body);
    }
    stream.write_all(request.as_bytes()).await.expect("write");

    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut raw))
        .await
        .expect("response deadline")
        .expect("read");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status: u16 = std::str::from_utf8(&raw[9..12])
        .expect("status slice")
        .parse()
        .expect("status line");
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();
    (status, body)
}

/// Starts an in-process server on an ephemeral port with one seeded secret.
async fn start_server() -> (std::net::SocketAddr, tempfile::TempDir, Arc<AppState>) {
    let dir = tempfile::tempdir().expect("temp dir");
    let vault = dir.path().join("vault.toml");
    let config =
        Config::from_parts(&vault, "master-pw", "eruka:tok-eruka", "127.0.0.1:0").expect("config");
    let state = Arc::new(AppState::new(&config).expect("state"));
    {
        let mut guard = state.vault();
        guard.set("api-key", VALUE, &["eruka"]).expect("seed");
        guard.save().expect("persist");
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server_state = Arc::clone(&state);
    tokio::spawn(async move {
        let _ = serve(listener, server_state).await;
    });
    (addr, dir, state)
}

#[tokio::test]
async fn in_process_server_handles_every_route_over_tcp() {
    let (addr, _dir, state) = start_server().await;

    // GET /healthz — open, no token.
    let (status, body) = round_trip(addr, "GET", "/healthz", None, None).await;
    assert_eq!(status, 200, "body: {body}");
    assert!(body.contains("\"status\":\"ok\""), "body: {body}");
    assert!(
        body.contains("\"version\""),
        "healthz must carry a version: {body}"
    );
    assert!(!body.contains(VALUE), "healthz leaked a value: {body}");
    assert!(
        !body.contains("api-key"),
        "healthz leaked vault metadata: {body}"
    );

    // PUT /api/v1/secrets/{key}
    let (status, body) = round_trip(
        addr,
        "PUT",
        "/api/v1/secrets/telegram-token",
        Some(TOKEN),
        Some(VALUE),
    )
    .await;
    assert_eq!(status, 200, "body: {body}");

    // GET /api/v1/secrets/{key} — the only route that returns a value.
    let (status, body) = round_trip(
        addr,
        "GET",
        "/api/v1/secrets/telegram-token",
        Some(TOKEN),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body, VALUE, "authorised GET must return exactly the value");

    // GET /api/v1/secrets — keys and tags only.
    let (status, body) = round_trip(addr, "GET", "/api/v1/secrets", Some(TOKEN), None).await;
    assert_eq!(status, 200);
    assert!(body.contains("telegram-token"), "body: {body}");
    assert!(body.contains("api-key"), "body: {body}");
    assert!(
        !body.contains(VALUE),
        "list response leaked a value: {body}"
    );
    let _ = &state;

    // DELETE /api/v1/secrets/{key}
    let (status, _) =
        round_trip(addr, "DELETE", "/api/v1/secrets/api-key", Some(TOKEN), None).await;
    assert_eq!(status, 200);
    let (status, _) = round_trip(addr, "GET", "/api/v1/secrets/api-key", Some(TOKEN), None).await;
    assert_eq!(status, 404, "deleted key must be gone");

    // Unknown key.
    let (status, _) = round_trip(addr, "GET", "/api/v1/secrets/nope", Some(TOKEN), None).await;
    assert_eq!(status, 404);
    let (status, _) = round_trip(addr, "DELETE", "/api/v1/secrets/nope", Some(TOKEN), None).await;
    assert_eq!(status, 404);

    // Unknown routes are never 200.
    for (method, path) in [
        ("GET", "/"),
        ("GET", "/api/v1"),
        ("GET", "/api/v1/secrets/"),
        ("GET", "/api/v1/secretz"),
        ("POST", "/api/v1/secrets"),
        ("DELETE", "/healthz"),
        ("GET", "/metrics"),
        ("PATCH", "/api/v1/secrets/api-key"),
    ] {
        let (status, body) = round_trip(addr, method, path, Some(TOKEN), None).await;
        assert_eq!(status, 404, "{method} {path} must be 404, got body: {body}");
    }
}

#[tokio::test]
async fn in_process_server_rejects_bad_bearer_tokens() {
    let (addr, _dir, _state) = start_server().await;
    let protected = [
        ("GET", "/api/v1/secrets"),
        ("GET", "/api/v1/secrets/api-key"),
        ("PUT", "/api/v1/secrets/new"),
        ("DELETE", "/api/v1/secrets/api-key"),
    ];
    for (method, path) in protected {
        let (status, _) = round_trip(addr, method, path, None, None).await;
        assert_eq!(status, 401, "{method} {path} with no token");
        let (status, _) = round_trip(addr, method, path, Some("wrong-token"), None).await;
        assert_eq!(status, 401, "{method} {path} with a wrong token");
    }

    // Malformed Authorization headers over the wire: bare token, empty bearer.
    for raw in ["tok-eruka", "Bearer tok-eruka-typo", "Bearer"] {
        let mut stream = TcpStream::connect(addr).await.expect("connect");
        let request = format!(
            "GET /api/v1/secrets HTTP/1.1\r\nHost: {addr}\r\nAuthorization: {raw}\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.expect("write");
        let mut raw_response = Vec::new();
        tokio::time::timeout(
            Duration::from_secs(10),
            stream.read_to_end(&mut raw_response),
        )
        .await
        .expect("deadline")
        .expect("read");
        let status: u16 = std::str::from_utf8(&raw_response[9..12])
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(status, 401, "Authorization: {raw:?} must be rejected");
    }

    // An empty bearer token must never be accepted.
    for raw in ["Bearer ", "Bearer   ", "Bearer \t"] {
        let mut stream = TcpStream::connect(addr).await.expect("connect");
        let request = format!(
            "GET /api/v1/secrets HTTP/1.1\r\nHost: {addr}\r\nAuthorization: {raw}\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.expect("write");
        let mut raw_response = Vec::new();
        tokio::time::timeout(
            Duration::from_secs(10),
            stream.read_to_end(&mut raw_response),
        )
        .await
        .expect("deadline")
        .expect("read");
        let status: u16 = std::str::from_utf8(&raw_response[9..12])
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(status, 401, "Authorization: {raw:?} must be rejected");
    }

    // Trailing whitespace is optional-whitespace (OWS) that the HTTP layer
    // strips before the handler ever sees it, so it still names the real token.
    // This is documented behaviour of the header parser, not an auth bypass:
    // the parser cannot manufacture a match for a token that was never sent.
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let request = format!(
        "GET /api/v1/secrets HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer tok-eruka \r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut raw_response = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(10),
        stream.read_to_end(&mut raw_response),
    )
    .await
    .expect("deadline")
    .expect("read");
    let status: u16 = std::str::from_utf8(&raw_response[9..12])
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        status, 200,
        "OWS-stripped token must match the configured one"
    );
}

// ---- The real binary refuses to start -------------------------------------

fn spawn_service(
    vault_password: Option<&str>,
    tokens: Option<&str>,
    vault_path: Option<&str>,
) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aegis-serve"));
    command.env_clear();
    // Never allow a test to fall back to the production defaults.
    command.env("AEGIS_VAULT_PASSWORD", "");
    command.env("AEGIS_TOKENS", "");
    command.env("AEGIS_VAULT_PATH", "");
    if let Some(password) = vault_password {
        command.env("AEGIS_VAULT_PASSWORD", password);
    }
    if let Some(tokens) = tokens {
        command.env("AEGIS_TOKENS", tokens);
    }
    if let Some(path) = vault_path {
        command.env("AEGIS_VAULT_PATH", path);
    }
    command
}

fn assert_refused(mut command: Command, needle: &str) {
    let output = command.output().expect("run binary");
    assert!(
        !output.status.success(),
        "the binary must exit non-zero; stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(needle),
        "stderr should name the failure ({needle}), got: {stderr}"
    );
}

#[test]
fn binary_refuses_to_start_without_password() {
    assert_refused(
        spawn_service(None, Some("eruka:s3cret"), None),
        "AEGIS_VAULT_PASSWORD",
    );
}

#[test]
fn binary_refuses_to_start_with_blank_password() {
    for password in ["", " ", "\t\n"] {
        assert_refused(
            spawn_service(Some(password), Some("eruka:s3cret"), None),
            "AEGIS_VAULT_PASSWORD",
        );
    }
}

#[test]
fn binary_refuses_to_start_without_tokens() {
    assert_refused(spawn_service(Some("master-pw"), None, None), "AEGIS_TOKENS");
}

#[test]
fn binary_refuses_to_start_with_unusable_tokens() {
    for tokens in ["", "  ", "eruka", ":tok", "eruka:", ","] {
        assert_refused(
            spawn_service(Some("master-pw"), Some(tokens), None),
            "AEGIS_TOKENS",
        );
    }
}

#[test]
fn binary_refuses_to_start_with_a_wrong_vault_password() {
    let dir = tempfile::tempdir().expect("temp dir");
    let vault = dir.path().join("vault.toml");
    {
        let mut store = aegis_secrets::store::SecretStore::open(&vault, "correct-horse").unwrap();
        store.set("api-key", VALUE, &["eruka"]).unwrap();
        store.save().unwrap();
    }
    let mut command = spawn_service(
        Some("wrong-horse"),
        Some("eruka:s3cret"),
        Some(vault.to_str().unwrap()),
    );
    let output = command.output().expect("run binary");
    assert!(
        !output.status.success(),
        "wrong password must exit non-zero: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains(VALUE),
        "the failure message must not contain the secret value: {stderr}"
    );
    assert!(
        stderr.contains("password"),
        "stderr should explain the vault failure: {stderr}"
    );
}
