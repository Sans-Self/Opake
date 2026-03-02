use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Json, Response};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier};

use crate::api::types::ErrorResponse;
use crate::state::AppState;

/// Maximum age of a signed request timestamp (replay protection).
const MAX_TIMESTAMP_DRIFT_SECS: i64 = 60;

/// Auth middleware: validates `Opake-Ed25519` DID-scoped signatures.
///
/// Header format:
///   Authorization: Opake-Ed25519 <did>:<unix-timestamp>:<base64(signature)>
/// Signature covers: <method>:<path>:<timestamp>:<did>
pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let method = req.method().as_str().to_string();
    let path = req.uri().path().to_string();
    let query_did = extract_did_param(req.uri().query());

    match auth_header.as_deref() {
        Some(h) if h.starts_with("Opake-Ed25519 ") => {
            match verify_did_auth(&state, h, &method, &path, query_did.as_deref()).await {
                Ok(()) => next.run(req).await,
                Err(msg) => unauthorized(&msg),
            }
        }
        Some(_) => unauthorized("unsupported authorization scheme — use Opake-Ed25519"),
        None => unauthorized("missing authorization header"),
    }
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
        .into_response()
}

/// Parse and verify an Opake-Ed25519 authorization header.
async fn verify_did_auth(
    state: &AppState,
    header: &str,
    method: &str,
    path: &str,
    query_did: Option<&str>,
) -> Result<(), String> {
    let payload = header
        .strip_prefix("Opake-Ed25519 ")
        .ok_or("malformed auth header")?;

    let (did, timestamp_str, sig_b64) = parse_auth_payload(payload)?;

    // Enforce: authenticated DID must match the ?did= query parameter
    if let Some(qd) = query_did {
        if qd != did {
            return Err(format!(
                "authenticated as {did} but requesting data for {qd}"
            ));
        }
    }

    // Validate timestamp (replay protection)
    let timestamp: i64 = timestamp_str
        .parse()
        .map_err(|_| "invalid timestamp in auth header")?;
    let now = chrono::Utc::now().timestamp();
    let drift = (now - timestamp).abs();
    if drift > MAX_TIMESTAMP_DRIFT_SECS {
        return Err(format!(
            "timestamp too far from current time ({drift}s drift, max {MAX_TIMESTAMP_DRIFT_SECS}s)"
        ));
    }

    // Decode signature
    let sig_bytes = BASE64
        .decode(sig_b64)
        .or_else(|_| {
            use base64::engine::general_purpose::STANDARD_NO_PAD;
            STANDARD_NO_PAD.decode(sig_b64)
        })
        .map_err(|_| "invalid base64 in signature")?;
    let signature =
        Signature::from_slice(&sig_bytes).map_err(|_| "invalid Ed25519 signature format")?;

    // Fetch/cache the signing key
    let verifying_key = state
        .key_cache
        .lock()
        .await
        .get_or_fetch(did)
        .await
        .map_err(|e| format!("failed to fetch signing key: {e}"))?;

    // Verify signature over: <method>:<path>:<timestamp>:<did>
    let message = format!("{method}:{path}:{timestamp_str}:{did}");
    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|_| "signature verification failed".to_string())
}

/// Parse the auth payload. DIDs contain colons, so we split from the right:
/// last segment = signature, second-to-last = timestamp, rest = DID.
fn parse_auth_payload(payload: &str) -> Result<(&str, &str, &str), String> {
    let last_colon = payload.rfind(':').ok_or("malformed auth payload")?;
    let sig = &payload[last_colon + 1..];
    let rest = &payload[..last_colon];

    let second_last = rest.rfind(':').ok_or("malformed auth payload")?;
    let timestamp = &rest[second_last + 1..];
    let did = &rest[..second_last];

    if did.is_empty() || timestamp.is_empty() || sig.is_empty() {
        return Err("malformed auth payload: empty component".into());
    }

    Ok((did, timestamp, sig))
}

/// Extract the `did` query parameter from a raw query string.
fn extract_did_param(query: Option<&str>) -> Option<String> {
    query.and_then(|q| {
        q.split('&')
            .find_map(|pair| pair.strip_prefix("did=").map(|v| v.to_string()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_auth_payload_valid() {
        let (did, ts, sig) = parse_auth_payload("did:plc:abc123:1709330400:c2lnbmF0dXJl").unwrap();
        assert_eq!(did, "did:plc:abc123");
        assert_eq!(ts, "1709330400");
        assert_eq!(sig, "c2lnbmF0dXJl");
    }

    #[test]
    fn parse_auth_payload_did_web() {
        let (did, ts, sig) = parse_auth_payload("did:web:example.com:1709330400:c2ln").unwrap();
        assert_eq!(did, "did:web:example.com");
        assert_eq!(ts, "1709330400");
        assert_eq!(sig, "c2ln");
    }

    #[test]
    fn parse_auth_payload_rejects_garbage() {
        assert!(parse_auth_payload("nocolons").is_err());
        assert!(parse_auth_payload("one:colon").is_err());
    }

    #[test]
    fn parse_auth_payload_rejects_empty_components() {
        assert!(parse_auth_payload("::sig").is_err());
        assert!(parse_auth_payload("did::sig").is_err());
    }

    #[test]
    fn extract_did_from_query() {
        assert_eq!(
            extract_did_param(Some("did=did:plc:abc&limit=10")),
            Some("did:plc:abc".into())
        );
        assert_eq!(extract_did_param(Some("limit=10")), None);
        assert_eq!(extract_did_param(None), None);
    }
}
