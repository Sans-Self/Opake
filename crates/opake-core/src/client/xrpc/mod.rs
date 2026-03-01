// XRPC client for talking to a PDS.
//
// Protocol logic for authenticated XRPC calls, generic over transport.
// Handles session management, token refresh, and the standard atproto
// repo operations (create/get/list/delete records, upload/get blobs).

mod auth;
mod blobs;
mod repo;

use serde::{Deserialize, Serialize};

use super::transport::*;
use crate::error::Error;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// An authenticated session with a PDS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub did: String,
    pub handle: String,
    pub access_jwt: String,
    pub refresh_jwt: String,
}

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

    let message = serde_json::from_slice::<XrpcError>(&response.body)
        .ok()
        .and_then(|e| match (e.error, e.message) {
            (Some(code), Some(msg)) => Some(format!("{code}: {msg}")),
            (Some(code), None) => Some(code),
            (None, Some(msg)) => Some(msg),
            (None, None) => None,
        })
        .unwrap_or_else(|| format!("HTTP {}", response.status));

    if response.status == 404 {
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

    /// Whether the session was refreshed during this client's lifetime.
    /// The CLI uses this to persist updated tokens to disk.
    pub fn session_refreshed(&self) -> bool {
        self.session_refreshed
    }

    fn auth_header(&self) -> Result<(String, String), Error> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| Error::Auth("not logged in".into()))?;
        Ok((
            "Authorization".into(),
            format!("Bearer {}", session.access_jwt),
        ))
    }

    fn did(&self) -> Result<&str, Error> {
        self.session
            .as_ref()
            .map(|s| s.did.as_str())
            .ok_or_else(|| Error::Auth("not logged in".into()))
    }

    /// Replace the Authorization header in a request with the current access token.
    fn replace_auth_header(&self, mut request: HttpRequest) -> Result<HttpRequest, Error> {
        let (key, value) = self.auth_header()?;
        if let Some(h) = request.headers.iter_mut().find(|(k, _)| k == &key) {
            h.1 = value;
        }
        Ok(request)
    }

    /// Check whether a PDS response is an expired-token error.
    fn is_expired_token(response: &HttpResponse) -> bool {
        if response.status != 400 {
            return false;
        }

        #[derive(Deserialize)]
        struct Body {
            error: Option<String>,
        }

        serde_json::from_slice::<Body>(&response.body)
            .ok()
            .and_then(|b| b.error)
            .is_some_and(|e| e == "ExpiredToken")
    }

    /// Send a request and check the response status. Every XRPC method except
    /// `login` (which has custom error handling) goes through here.
    ///
    /// If the PDS returns `ExpiredToken`, the session is automatically refreshed
    /// and the request is retried once with the new access token.
    async fn send_checked(&mut self, request: HttpRequest) -> Result<HttpResponse, Error> {
        let mut response = self.transport.send(request.clone()).await?;

        if Self::is_expired_token(&response) {
            self.refresh_session().await?;
            let retried = self.replace_auth_header(request)?;
            response = self.transport.send(retried).await?;
        }

        check_response(&response)?;
        Ok(response)
    }
}

#[cfg(test)]
#[path = "xrpc_tests.rs"]
mod tests;
