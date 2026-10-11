//! Per-agent bearer-token authentication.
//!
//! Tokens are configured at startup as a comma-separated list of
//! `agent:token` pairs. Requests to `/api/v1/…` must present one of those
//! tokens in `Authorization: Bearer <token>`. Everything else — a missing
//! header, a malformed header, an empty token, or a token that matches nothing —
//! is rejected. The map has no notion of a "default" token, so an empty token
//! set can never admit a request.

use std::collections::HashMap;

use hyper::http::HeaderMap;

/// Parses `agent:token,agent:token` into a map keyed by agent name.
///
/// The rules are deliberately strict: each pair must have a non-empty agent
/// name and a non-empty token value, or the pair is discarded. Parsing never
/// fails loudly — callers treat an empty result as "no agent may
/// authenticate", which is the safe direction.
pub fn parse_tokens(raw: &str) -> HashMap<String, String> {
    let mut tokens = HashMap::new();
    for pair in raw.split(',') {
        let Some((agent, token)) = pair.split_once(':') else {
            continue;
        };
        let agent = agent.trim();
        let token = token.trim();
        if agent.is_empty() || token.is_empty() {
            continue;
        }
        tokens.insert(agent.to_string(), token.to_string());
    }
    tokens
}

/// Extracts a well-formed bearer token from an `Authorization` header.
///
/// Returns `None` for a missing header, non-ASCII header value, a missing
/// `Bearer ` prefix, or an empty token — every case the caller must treat as
/// unauthorized rather than trying to match.
pub fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(hyper::http::header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix(super::state::BEARER_PREFIX)?;
    if token.is_empty() || token.trim().is_empty() {
        return None;
    }
    Some(token)
}

/// Reports whether the request's bearer token is one of the configured tokens.
///
/// The comparison runs over every candidate so a token is only accepted on an
/// actual match; a wrong-length candidate exits early, an equal-length
/// candidate is compared without early exit on first difference.
pub fn is_authorized(tokens: &HashMap<String, String>, headers: &HeaderMap) -> bool {
    let Some(provided) = bearer_token(headers) else {
        return false;
    };
    tokens
        .values()
        .any(|configured| constant_time_eq(configured.as_bytes(), provided.as_bytes()))
}

/// Byte comparison that does not short-circuit on the first differing byte.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with_token(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            hyper::http::header::AUTHORIZATION,
            hyper::http::HeaderValue::from_str(value).unwrap(),
        );
        headers
    }

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(agent, token)| ((*agent).to_string(), (*token).to_string()))
            .collect()
    }

    #[test]
    fn parses_pairs_and_ignores_malformed_entries() {
        let parsed = parse_tokens("eruka:s3cret, ares:other-token");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed["eruka"], "s3cret");
        assert_eq!(parsed["ares"], "other-token");
        // Trimming must not swallow a token that contains a colon or a dash.
        let with_colon = parse_tokens("ares:pass:word-1");
        assert_eq!(with_colon["ares"], "pass:word-1");
    }

    #[test]
    fn malformed_pairs_are_dropped_not_guessed() {
        for raw in ["", "   ", "eruka", "eruka:", ":only-token", " , , ", "eruka: ,ares:t"] {
            let parsed = parse_tokens(raw);
            if raw == "eruka: ,ares:t" {
                assert_eq!(parsed.len(), 1, "only the valid pair should survive");
                assert_eq!(parsed["ares"], "t");
            } else {
                assert!(parsed.is_empty(), "{raw:?} must not yield any token");
            }
        }
    }

    #[test]
    fn a_valid_token_is_accepted() {
        let tokens = map(&[("eruka", "s3cret"), ("ares", "other")]);
        assert!(is_authorized(&tokens, &headers_with_token("Bearer s3cret")));
        assert!(is_authorized(&tokens, &headers_with_token("Bearer other")));
    }

    #[test]
    fn every_bad_authorization_shape_is_rejected() {
        let tokens = map(&[("eruka", "s3cret")]);
        for bad in [
            "",                        // no header value at all
            "s3cret",                  // bare token, no scheme
            "Bearer s3cret-typo",      // wrong token
            "Bearer s3cret-with-suffix", // right prefix, extra bytes
            "bearer s3cret",           // lowercase scheme
            "BEARER s3cret",           // uppercase scheme
            "Bearer  ",                // whitespace-only token
            "Basic c3NyZWN0",          // wrong scheme, correct base64 of s3cret
            "Bearer s3cret ",          // trailing space makes the token longer
        ] {
            let headers = if bad.is_empty() {
                HeaderMap::new()
            } else {
                headers_with_token(bad)
            };
            assert!(
                !is_authorized(&tokens, &headers),
                "{bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn empty_token_map_rejects_every_shape() {
        let empty = map(&[]);
        for shape in ["Bearer ", "Bearer x", "Bearer s3cret", "Bearer \t"] {
            let headers = headers_with_token(shape);
            assert!(
                !is_authorized(&empty, &headers),
                "empty token map must reject {shape:?}"
            );
        }
    }

    #[test]
    fn constant_time_eq_compares_whole_string() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(!constant_time_eq(b"abc", b""));
        // A candidate that differs only in the final byte must not be accepted,
        // which a short-circuiting comparison would return instantly but wrongly.
        assert!(!constant_time_eq(b"s3cret", b"s3cre+"));
    }
}
