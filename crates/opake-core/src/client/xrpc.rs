// XRPC client for talking to a PDS.
//
// Protocol logic for authenticated XRPC calls, generic over transport.
// Handles session management, token refresh, and the standard atproto
// repo operations (create/get/list/delete records, upload/get blobs).

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};

use super::transport::*;
use crate::atproto::BlobRef;
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

    /// Authenticate via `com.atproto.server.createSession`.
    pub async fn login(&mut self, identifier: &str, password: &str) -> Result<&Session, Error> {
        info!("authenticating as {} against {}", identifier, self.base_url);

        let body = serde_json::json!({
            "identifier": identifier,
            "password": password,
        });

        let response = self
            .transport
            .send(HttpRequest {
                method: HttpMethod::Post,
                url: format!("{}/xrpc/com.atproto.server.createSession", self.base_url),
                headers: vec![("Content-Type".into(), "application/json".into())],
                body: Some(RequestBody::Json(body)),
            })
            .await?;

        if response.status != 200 {
            warn!("login failed with HTTP {}", response.status);
            return Err(Error::Auth(format!(
                "login failed (HTTP {})",
                response.status
            )));
        }

        let session: Session = serde_json::from_slice(&response.body)?;
        info!("authenticated as {} ({})", session.handle, session.did);
        self.session = Some(session);
        Ok(self.session.as_ref().unwrap())
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

    /// Refresh the session using the stored refresh_jwt.
    async fn refresh_session(&mut self) -> Result<(), Error> {
        let refresh_jwt = self
            .session
            .as_ref()
            .map(|s| s.refresh_jwt.clone())
            .ok_or_else(|| Error::Auth("not logged in".into()))?;

        info!("access token expired, refreshing session");

        let response = self
            .transport
            .send(HttpRequest {
                method: HttpMethod::Post,
                url: format!("{}/xrpc/com.atproto.server.refreshSession", self.base_url),
                headers: vec![("Authorization".into(), format!("Bearer {}", refresh_jwt))],
                body: None,
            })
            .await?;

        if response.status != 200 {
            warn!("session refresh failed with HTTP {}", response.status);
            return Err(Error::Auth(format!(
                "session refresh failed (HTTP {}) — run `opake login` again",
                response.status
            )));
        }

        let new_session: Session = serde_json::from_slice(&response.body)?;
        info!("session refreshed for {}", new_session.handle);
        self.session = Some(new_session);
        self.session_refreshed = true;

        Ok(())
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

    /// Upload raw bytes as a blob via `com.atproto.repo.uploadBlob`.
    pub async fn upload_blob(&mut self, data: Vec<u8>, mime_type: &str) -> Result<BlobRef, Error> {
        debug!("uploading blob ({} bytes, {})", data.len(), mime_type);
        let auth = self.auth_header()?;

        let response = self
            .send_checked(HttpRequest {
                method: HttpMethod::Post,
                url: format!("{}/xrpc/com.atproto.repo.uploadBlob", self.base_url),
                headers: vec![auth, ("Content-Type".into(), mime_type.into())],
                body: Some(RequestBody::Bytes {
                    data,
                    content_type: mime_type.into(),
                }),
            })
            .await?;

        #[derive(Deserialize)]
        struct UploadResponse {
            blob: BlobRef,
        }

        let parsed: UploadResponse = serde_json::from_slice(&response.body)?;
        Ok(parsed.blob)
    }

    /// Fetch a blob by DID + CID via `com.atproto.sync.getBlob`.
    pub async fn get_blob(&mut self, did: &str, cid: &str) -> Result<Vec<u8>, Error> {
        debug!("fetching blob did={} cid={}", did, cid);
        let auth = self.auth_header()?;
        let url = format!(
            "{}/xrpc/com.atproto.sync.getBlob?did={}&cid={}",
            self.base_url, did, cid,
        );

        let response = self
            .send_checked(HttpRequest {
                method: HttpMethod::Get,
                url,
                headers: vec![auth],
                body: None,
            })
            .await?;

        Ok(response.body)
    }

    /// Create a record via `com.atproto.repo.createRecord`.
    pub async fn create_record<R: Serialize>(
        &mut self,
        collection: &str,
        record: &R,
    ) -> Result<RecordRef, Error> {
        debug!("creating record in {}", collection);
        let auth = self.auth_header()?;
        let did = self.did()?;

        let body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "record": record,
        });

        let response = self
            .send_checked(HttpRequest {
                method: HttpMethod::Post,
                url: format!("{}/xrpc/com.atproto.repo.createRecord", self.base_url),
                headers: vec![auth, ("Content-Type".into(), "application/json".into())],
                body: Some(RequestBody::Json(body)),
            })
            .await?;

        Ok(serde_json::from_slice(&response.body)?)
    }

    /// Upsert a record with an explicit rkey via `com.atproto.repo.putRecord`.
    ///
    /// Idempotent — creates or overwrites the record at `collection/rkey`.
    /// Used for singleton records like `app.opake.cloud.publicKey/self`.
    pub async fn put_record<R: Serialize>(
        &mut self,
        collection: &str,
        rkey: &str,
        record: &R,
    ) -> Result<RecordRef, Error> {
        debug!("putting record {}/{}", collection, rkey);
        let auth = self.auth_header()?;
        let did = self.did()?;

        let body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "rkey": rkey,
            "record": record,
        });

        let response = self
            .send_checked(HttpRequest {
                method: HttpMethod::Post,
                url: format!("{}/xrpc/com.atproto.repo.putRecord", self.base_url),
                headers: vec![auth, ("Content-Type".into(), "application/json".into())],
                body: Some(RequestBody::Json(body)),
            })
            .await?;

        Ok(serde_json::from_slice(&response.body)?)
    }

    /// Fetch a single record via `com.atproto.repo.getRecord`.
    pub async fn get_record(
        &mut self,
        did: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<RecordEntry, Error> {
        debug!("getting record {}/{}/{}", did, collection, rkey);
        let auth = self.auth_header()?;
        let url = format!(
            "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey={}",
            self.base_url, did, collection, rkey,
        );

        let response = self
            .send_checked(HttpRequest {
                method: HttpMethod::Get,
                url,
                headers: vec![auth],
                body: None,
            })
            .await?;

        Ok(serde_json::from_slice(&response.body)?)
    }

    /// List records in a collection via `com.atproto.repo.listRecords`.
    pub async fn list_records(
        &mut self,
        collection: &str,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<RecordPage, Error> {
        debug!("listing records in {}", collection);
        let auth = self.auth_header()?;
        let did = self.did()?;

        let mut url = format!(
            "{}/xrpc/com.atproto.repo.listRecords?repo={}&collection={}",
            self.base_url, did, collection,
        );
        if let Some(limit) = limit {
            url.push_str(&format!("&limit={}", limit));
        }
        if let Some(cursor) = cursor {
            url.push_str(&format!("&cursor={}", cursor));
        }

        let response = self
            .send_checked(HttpRequest {
                method: HttpMethod::Get,
                url,
                headers: vec![auth],
                body: None,
            })
            .await?;

        Ok(serde_json::from_slice(&response.body)?)
    }

    /// Delete a record via `com.atproto.repo.deleteRecord`.
    pub async fn delete_record(&mut self, collection: &str, rkey: &str) -> Result<(), Error> {
        debug!("deleting record {}/{}", collection, rkey);
        let auth = self.auth_header()?;
        let did = self.did()?;

        let body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "rkey": rkey,
        });

        self.send_checked(HttpRequest {
            method: HttpMethod::Post,
            url: format!("{}/xrpc/com.atproto.repo.deleteRecord", self.base_url),
            headers: vec![auth, ("Content-Type".into(), "application/json".into())],
            body: Some(RequestBody::Json(body)),
        })
        .await?;

        Ok(())
    }
}

#[cfg(test)]
#[path = "xrpc_tests.rs"]
mod tests;
