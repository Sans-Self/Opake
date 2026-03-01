// XRPC client for talking to a PDS.
//
// Transport is injected via trait — the CLI provides a reqwest-based
// implementation, the SPA (opake-web) provides one backed by browser fetch.
// Core owns the XRPC protocol logic (endpoints, auth, response parsing)
// but never touches the network directly.

use log::{debug, info, warn};

use crate::atproto::BlobRef;
use crate::error::Error;
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

    /// Send a request and check the response status. Every XRPC method except
    /// `login` (which has custom error handling) goes through here.
    async fn send_checked(&self, request: HttpRequest) -> Result<HttpResponse, Error> {
        let response = self.transport.send(request).await?;
        Self::check_response(&response)?;
        Ok(response)
    }

    /// Check an XRPC response for errors. Non-2xx responses are parsed as
    /// XRPC error bodies (`{"error":"...", "message":"..."}`) when possible,
    /// falling back to the raw status code.
    fn check_response(response: &HttpResponse) -> Result<(), Error> {
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

    /// Upload raw bytes as a blob via `com.atproto.repo.uploadBlob`.
    pub async fn upload_blob(&self, data: Vec<u8>, mime_type: &str) -> Result<BlobRef, Error> {
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
    pub async fn get_blob(&self, did: &str, cid: &str) -> Result<Vec<u8>, Error> {
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
        &self,
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

    /// Fetch a single record via `com.atproto.repo.getRecord`.
    pub async fn get_record(
        &self,
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
        &self,
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
    pub async fn delete_record(&self, collection: &str, rkey: &str) -> Result<(), Error> {
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
mod tests {
    use super::*;

    // Test check_response directly — it's pure logic over HttpResponse,
    // no transport needed.

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            body: body.as_bytes().to_vec(),
        }
    }

    // -- 2xx success range --

    #[test]
    fn ok_200_passes() {
        assert!(XrpcClient::<DummyTransport>::check_response(&response(200, "")).is_ok());
    }

    #[test]
    fn created_201_passes() {
        assert!(XrpcClient::<DummyTransport>::check_response(&response(201, "")).is_ok());
    }

    #[test]
    fn no_content_204_passes() {
        assert!(XrpcClient::<DummyTransport>::check_response(&response(204, "")).is_ok());
    }

    // -- XRPC error bodies --

    #[test]
    fn error_500_with_xrpc_body() {
        let r = response(
            500,
            r#"{"error":"InternalServerError","message":"Internal Server Error"}"#,
        );
        let err = XrpcClient::<DummyTransport>::check_response(&r).unwrap_err();
        match err {
            Error::Xrpc { status, message } => {
                assert_eq!(status, 500);
                assert!(message.contains("InternalServerError"));
                assert!(message.contains("Internal Server Error"));
            }
            other => panic!("expected Xrpc error, got: {other}"),
        }
    }

    #[test]
    fn error_400_with_error_code_only() {
        let r = response(400, r#"{"error":"InvalidRequest"}"#);
        let err = XrpcClient::<DummyTransport>::check_response(&r).unwrap_err();
        match err {
            Error::Xrpc { status, message } => {
                assert_eq!(status, 400);
                assert_eq!(message, "InvalidRequest");
            }
            other => panic!("expected Xrpc error, got: {other}"),
        }
    }

    #[test]
    fn error_403_with_message_only() {
        let r = response(403, r#"{"message":"not authorized"}"#);
        let err = XrpcClient::<DummyTransport>::check_response(&r).unwrap_err();
        match err {
            Error::Xrpc { status, message } => {
                assert_eq!(status, 403);
                assert_eq!(message, "not authorized");
            }
            other => panic!("expected Xrpc error, got: {other}"),
        }
    }

    // -- 404 maps to NotFound --

    #[test]
    fn error_404_returns_not_found() {
        let r = response(
            404,
            r#"{"error":"RecordNotFound","message":"no such record"}"#,
        );
        let err = XrpcClient::<DummyTransport>::check_response(&r).unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    // -- Non-JSON error bodies --

    #[test]
    fn error_502_with_html_body() {
        let r = response(502, "<html><body>Bad Gateway</body></html>");
        let err = XrpcClient::<DummyTransport>::check_response(&r).unwrap_err();
        match err {
            Error::Xrpc { status, message } => {
                assert_eq!(status, 502);
                assert_eq!(message, "HTTP 502");
            }
            other => panic!("expected Xrpc error, got: {other}"),
        }
    }

    #[test]
    fn error_500_with_empty_body() {
        let r = response(500, "");
        let err = XrpcClient::<DummyTransport>::check_response(&r).unwrap_err();
        match err {
            Error::Xrpc { status, message } => {
                assert_eq!(status, 500);
                assert_eq!(message, "HTTP 500");
            }
            other => panic!("expected Xrpc error, got: {other}"),
        }
    }

    #[test]
    fn error_500_with_empty_json_object() {
        let r = response(500, "{}");
        let err = XrpcClient::<DummyTransport>::check_response(&r).unwrap_err();
        match err {
            Error::Xrpc { status, message } => {
                assert_eq!(status, 500);
                assert_eq!(message, "HTTP 500");
            }
            other => panic!("expected Xrpc error, got: {other}"),
        }
    }

    // -- Edge: 3xx is not success --

    #[test]
    fn redirect_300_is_error() {
        assert!(XrpcClient::<DummyTransport>::check_response(&response(300, "")).is_err());
    }

    // -- Dummy transport for type parameter (never called) --

    struct DummyTransport;

    impl Transport for DummyTransport {
        async fn send(&self, _request: HttpRequest) -> Result<HttpResponse, Error> {
            unreachable!("check_response tests don't use transport")
        }
    }
}
