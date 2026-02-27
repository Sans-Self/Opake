// XRPC client for talking to a PDS.
//
// Transport is injected via trait — the CLI provides a reqwest-based
// implementation, the SPA (opake-web) provides one backed by browser fetch.
// Core owns the XRPC protocol logic (endpoints, auth, response parsing)
// but never touches the network directly.

use crate::error::Error;
use crate::records::BlobRef;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Transport trait — the injectable I/O boundary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone)]
pub enum RequestBody {
    Json(serde_json::Value),
    Bytes { data: Vec<u8>, content_type: String },
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<RequestBody>,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// The only thing a platform needs to provide: send an HTTP request, get bytes back.
/// CLI implements this with reqwest, the SPA with browser fetch via web_sys.
pub trait Transport {
    fn send(
        &self,
        request: HttpRequest,
    ) -> impl std::future::Future<Output = Result<HttpResponse, Error>>;
}

// ---------------------------------------------------------------------------
// XRPC client — protocol logic, generic over transport
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

pub struct XrpcClient<T: Transport> {
    transport: T,
    base_url: String,
    session: Option<Session>,
}

impl<T: Transport> XrpcClient<T> {
    pub fn new(transport: T, base_url: String) -> Self {
        Self {
            transport,
            base_url,
            session: None,
        }
    }

    pub fn with_session(transport: T, base_url: String, session: Session) -> Self {
        Self {
            transport,
            base_url,
            session: Some(session),
        }
    }

    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// Authenticate via `com.atproto.server.createSession`.
    pub async fn login(&mut self, identifier: &str, password: &str) -> Result<&Session, Error> {
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
            return Err(Error::Auth(format!(
                "login failed (HTTP {})",
                response.status
            )));
        }

        let session: Session = serde_json::from_slice(&response.body)?;
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

    /// Upload raw bytes as a blob via `com.atproto.repo.uploadBlob`.
    pub async fn upload_blob(&self, data: Vec<u8>, mime_type: &str) -> Result<BlobRef, Error> {
        let auth = self.auth_header()?;

        let response = self
            .transport
            .send(HttpRequest {
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
    pub async fn get_blob(&self, did: &str, cid: &str) -> Result<Vec<u8>, Error> {
        let auth = self.auth_header()?;
        let url = format!(
            "{}/xrpc/com.atproto.sync.getBlob?did={}&cid={}",
            self.base_url, did, cid,
        );

        let response = self
            .transport
            .send(HttpRequest {
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
        &self,
        collection: &str,
        record: &R,
    ) -> Result<RecordRef, Error> {
        let auth = self.auth_header()?;
        let did = self.did()?;

        let body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "record": record,
        });

        let response = self
            .transport
            .send(HttpRequest {
                method: HttpMethod::Post,
                url: format!("{}/xrpc/com.atproto.repo.createRecord", self.base_url),
                headers: vec![auth, ("Content-Type".into(), "application/json".into())],
                body: Some(RequestBody::Json(body)),
            })
            .await?;

        Ok(serde_json::from_slice(&response.body)?)
    }

    /// Fetch a single record via `com.atproto.repo.getRecord`.
    pub async fn get_record(
        &self,
        did: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<RecordEntry, Error> {
        let auth = self.auth_header()?;
        let url = format!(
            "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey={}",
            self.base_url, did, collection, rkey,
        );

        let response = self
            .transport
            .send(HttpRequest {
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
        &self,
        collection: &str,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<RecordPage, Error> {
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
            .transport
            .send(HttpRequest {
                method: HttpMethod::Get,
                url,
                headers: vec![auth],
                body: None,
            })
            .await?;

        Ok(serde_json::from_slice(&response.body)?)
    }

    /// Delete a record via `com.atproto.repo.deleteRecord`.
    pub async fn delete_record(&self, collection: &str, rkey: &str) -> Result<(), Error> {
        let auth = self.auth_header()?;
        let did = self.did()?;

        let body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "rkey": rkey,
        });

        self.transport
            .send(HttpRequest {
                method: HttpMethod::Post,
                url: format!("{}/xrpc/com.atproto.repo.deleteRecord", self.base_url),
                headers: vec![auth, ("Content-Type".into(), "application/json".into())],
                body: Some(RequestBody::Json(body)),
            })
            .await?;

        Ok(())
    }
}
