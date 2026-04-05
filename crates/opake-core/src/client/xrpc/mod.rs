// XRPC client for talking to a PDS.
//
// Protocol logic for authenticated XRPC calls, generic over transport.
// Handles session management, token refresh, and the standard atproto
// repo operations (create/get/list/delete records, upload/get blobs).

mod auth;
mod blobs;
mod repo;

pub use repo::ApplyWriteOp;

use serde::{Deserialize, Serialize};

use super::dpop::{create_dpop_proof, extract_dpop_nonce, is_use_dpop_nonce_error, DpopKeyPair};
use super::transport::*;
use crate::crypto::OsRng;
use crate::error::Error;

// ---------------------------------------------------------------------------
// Session types — discriminated union
// ---------------------------------------------------------------------------

/// An authenticated session with a PDS. Either legacy (password-based Bearer
/// tokens) or OAuth (DPoP-bound tokens).
///
/// Custom deserializer: JSON without a `"type"` field deserializes as `Legacy`
/// for backward compat with existing session.json files.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum Session {
    #[serde(rename = "legacy")]
    Legacy(LegacySession),
    #[serde(rename = "oauth")]
    OAuth(OAuthSession),
}

/// Legacy password-based session (createSession / refreshSession).
#[derive(Clone, crate::RedactedDebug, Serialize, Deserialize)]
pub struct LegacySession {
    pub did: String,
    pub handle: String,
    #[redact]
    pub access_jwt: String,
    #[redact]
    pub refresh_jwt: String,
}

/// OAuth 2.0 + DPoP session.
#[derive(Clone, crate::RedactedDebug, Serialize, Deserialize)]
pub struct OAuthSession {
    pub did: String,
    pub handle: String,
    #[redact]
    pub access_token: String,
    #[redact]
    pub refresh_token: String,
    pub dpop_key: DpopKeyPair,
    pub token_endpoint: String,
    #[serde(default)]
    pub dpop_nonce: Option<String>,
    /// Unix timestamp when the access token expires.
    #[serde(default)]
    pub expires_at: Option<i64>,
    pub client_id: String,
}

impl OAuthSession {
    /// Apply a token response to this session, updating tokens and expiry.
    /// Shared by both the reactive refresh (XrpcClient) and proactive refresh (daemon/SW).
    pub fn apply_token_response(
        &mut self,
        response: &crate::client::oauth_token::TokenResponse,
        now: i64,
    ) {
        self.access_token.clone_from(&response.access_token);
        if let Some(ref rt) = response.refresh_token {
            self.refresh_token.clone_from(rt);
        }
        if let Some(expires_in) = response.expires_in {
            self.expires_at = Some(now + expires_in as i64);
        }
    }
}

impl Session {
    pub fn did(&self) -> &str {
        match self {
            Session::Legacy(s) => &s.did,
            Session::OAuth(s) => &s.did,
        }
    }

    pub fn handle(&self) -> &str {
        match self {
            Session::Legacy(s) => &s.handle,
            Session::OAuth(s) => &s.handle,
        }
    }

    /// Unix timestamp when the access token expires, if known.
    pub fn expires_at(&self) -> Option<i64> {
        match self {
            Session::Legacy(_) => None,
            Session::OAuth(s) => s.expires_at,
        }
    }

    /// Whether the session's access token will expire within `threshold_seconds`.
    ///
    /// Returns `true` if:
    /// - OAuth session with `expires_at` set and within threshold of `now`
    /// - OAuth session without `expires_at` (unknown expiry — refresh to be safe)
    /// - Legacy session (always true — no expiry info, refresh is a health check)
    pub fn needs_refresh(&self, threshold_seconds: i64, now: i64) -> bool {
        match self {
            Session::Legacy(_) => true,
            Session::OAuth(s) => match s.expires_at {
                Some(expires_at) => now + threshold_seconds >= expires_at,
                None => true,
            },
        }
    }
}

/// Custom deserializer: if the JSON has a `"type"` field, use it as the tag.
/// If it doesn't (old session.json files), assume Legacy.
impl<'de> Deserialize<'de> for Session {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Use serde's built-in tagged enum support instead of routing
        // through serde_json::Value. The old approach broke when the
        // deserializer was serde_wasm_bindgen (JS→Rust), because
        // serde_json::Value::deserialize from a non-JSON deserializer
        // is a lossy conversion.
        //
        // Backward compat: files without a "type" field are treated as
        // legacy sessions via the #[serde(other)] fallback below.

        #[derive(Deserialize)]
        #[serde(tag = "type")]
        enum Tagged {
            #[serde(rename = "oauth")]
            OAuth(OAuthSession),
            #[serde(rename = "legacy")]
            Legacy(LegacySession),
        }

        // Try tagged first (has "type" field)
        // For backward compat with legacy sessions that lack "type",
        // use an untagged fallback.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Compat {
            Tagged(Tagged),
            LegacyFallback(LegacySession),
        }

        match Compat::deserialize(deserializer)? {
            Compat::Tagged(Tagged::OAuth(s)) => Ok(Session::OAuth(s)),
            Compat::Tagged(Tagged::Legacy(s)) | Compat::LegacyFallback(s) => {
                Ok(Session::Legacy(s))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Other record types
// ---------------------------------------------------------------------------

/// Reference to a created record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordRef {
    pub uri: String,
    pub cid: String,
}

/// A page of records from `listRecords`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordPage {
    pub records: Vec<RecordEntry>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordEntry {
    pub uri: String,
    pub cid: String,
    pub value: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Response checking — used by both XrpcClient and the DID/public functions
// ---------------------------------------------------------------------------

/// Check an XRPC response for errors. Non-2xx responses are parsed as
/// XRPC error bodies (`{"error":"...", "message":"..."}`) when possible,
/// falling back to the raw status code.
pub fn check_response(response: &HttpResponse) -> Result<(), Error> {
    if (200..300).contains(&response.status) {
        return Ok(());
    }

    #[derive(Deserialize)]
    struct XrpcError {
        error: Option<String>,
        message: Option<String>,
    }

    let parsed = serde_json::from_slice::<XrpcError>(&response.body).ok();

    let error_code = parsed.as_ref().and_then(|e| e.error.clone());

    let message = parsed
        .and_then(|e| match (e.error, e.message) {
            (Some(code), Some(msg)) => Some(format!("{code}: {msg}")),
            (Some(code), None) => Some(code),
            (None, Some(msg)) => Some(msg),
            (None, None) => None,
        })
        .unwrap_or_else(|| format!("HTTP {}", response.status));

    // 404 is the standard "not found" status. Some PDS implementations
    // (e.g. Tranquil) return 400 with a *NotFound error code instead.
    let is_not_found = response.status == 404
        || error_code
            .as_deref()
            .is_some_and(|c| c.ends_with("NotFound"));

    if is_not_found {
        Err(Error::NotFound(message))
    } else {
        Err(Error::Xrpc {
            status: response.status,
            message,
        })
    }
}

// ---------------------------------------------------------------------------
// XrpcClient
// ---------------------------------------------------------------------------

pub struct XrpcClient<T: Transport> {
    transport: T,
    base_url: String,
    session: Option<Session>,
    session_refreshed: bool,
}

#[cfg(feature = "reqwest-transport")]
impl XrpcClient<super::ReqwestTransport> {
    pub fn reqwest(base_url: String) -> Self {
        Self::new(super::ReqwestTransport::new(), base_url)
    }

    pub fn reqwest_with_session(base_url: String, session: Session) -> Self {
        Self::with_session(super::ReqwestTransport::new(), base_url, session)
    }
}

impl<T: Transport> XrpcClient<T> {
    pub fn new(transport: T, base_url: String) -> Self {
        Self {
            transport,
            base_url,
            session: None,
            session_refreshed: false,
        }
    }

    pub fn with_session(transport: T, base_url: String, session: Session) -> Self {
        Self {
            transport,
            base_url,
            session: Some(session),
            session_refreshed: false,
        }
    }

    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// Access the underlying transport for unauthenticated requests
    /// (e.g. cross-PDS DID resolution, public record fetches).
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// The PDS base URL this client is connected to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Whether the session was refreshed during this client's lifetime.
    /// The CLI uses this to persist updated tokens to disk.
    pub fn session_refreshed(&self) -> bool {
        self.session_refreshed
    }

    /// Attach auth headers to a request, dispatching on session variant.
    /// Legacy: `Authorization: Bearer <access_jwt>`
    /// OAuth: `Authorization: DPoP <access_token>` + `DPoP: <proof>`
    fn attach_auth(&mut self, request: &mut HttpRequest) -> Result<(), Error> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| Error::Auth("not logged in".into()))?;

        match session {
            Session::Legacy(s) => {
                request
                    .headers
                    .push(("Authorization".into(), format!("Bearer {}", s.access_jwt)));
            }
            Session::OAuth(s) => {
                let method = match request.method {
                    HttpMethod::Get => "GET",
                    HttpMethod::Post => "POST",
                };
                let timestamp = super::time::unix_now();
                let proof = create_dpop_proof(
                    &s.dpop_key,
                    method,
                    &request.url,
                    timestamp,
                    s.dpop_nonce.as_deref(),
                    Some(&s.access_token),
                    &mut OsRng,
                )?;
                request
                    .headers
                    .push(("Authorization".into(), format!("DPoP {}", s.access_token)));
                request.headers.push(("DPoP".into(), proof));
            }
        }

        Ok(())
    }

    pub fn did(&self) -> Result<&str, Error> {
        self.session
            .as_ref()
            .map(|s| s.did())
            .ok_or_else(|| Error::Auth("not logged in".into()))
    }

    /// Replace auth headers in a request with current credentials.
    fn replace_auth_headers(&mut self, mut request: HttpRequest) -> Result<HttpRequest, Error> {
        // Remove existing auth headers
        request.headers.retain(|(k, _)| {
            !k.eq_ignore_ascii_case("authorization") && !k.eq_ignore_ascii_case("dpop")
        });
        self.attach_auth(&mut request)?;
        Ok(request)
    }

    /// Capture DPoP-Nonce from response and update OAuth session if present.
    fn update_dpop_nonce(&mut self, response: &HttpResponse) {
        if let Some(Session::OAuth(ref mut s)) = self.session {
            if let Some(nonce) = extract_dpop_nonce(response) {
                if s.dpop_nonce.as_deref() != Some(&nonce) {
                    s.dpop_nonce = Some(nonce);
                    self.session_refreshed = true;
                }
            }
        }
    }

    /// Check whether a PDS response is an expired-token error.
    /// The AT Protocol PDS may return ExpiredToken on either 400 or 401.
    fn is_expired_token(response: &HttpResponse) -> bool {
        if response.status != 400 && response.status != 401 {
            return false;
        }

        #[derive(Deserialize)]
        struct Body {
            error: Option<String>,
        }

        serde_json::from_slice::<Body>(&response.body)
            .ok()
            .and_then(|b| b.error)
            .is_some_and(|e| e == "ExpiredToken" || e == "AuthenticationFailed")
    }

    /// Send a request and check the response status. Every XRPC method except
    /// `login` (which has custom error handling) goes through here.
    ///
    /// If the PDS returns `ExpiredToken`, the session is automatically refreshed
    /// and the request is retried once with the new access token.
    async fn send_checked(&mut self, request: HttpRequest) -> Result<HttpResponse, Error> {
        let mut response = self.transport.send(request.clone()).await?;
        self.update_dpop_nonce(&response);

        // The PDS may reject the first request for a stale DPoP nonce (the AS
        // nonce doesn't work here). Retry with the PDS-provided nonce.
        if is_use_dpop_nonce_error(&response) {
            let retried = self.replace_auth_headers(request.clone())?;
            response = self.transport.send(retried).await?;
            self.update_dpop_nonce(&response);
        }

        // After a nonce fix (or on the first attempt), the token itself may be
        // expired. Refresh and retry. A second nonce error here is not retried —
        // one retry is the spec-expected behavior (RFC 9449 §7.1).
        if Self::is_expired_token(&response) {
            self.refresh_session().await?;
            let retried = self.replace_auth_headers(request)?;
            response = self.transport.send(retried).await?;
            self.update_dpop_nonce(&response);
        }

        check_response(&response)?;
        Ok(response)
    }
}

#[cfg(test)]
#[path = "xrpc_tests.rs"]
mod tests;
