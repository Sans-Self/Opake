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
mod tests {
    use super::*;
    use crate::test_utils::MockTransport;

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            body: body.as_bytes().to_vec(),
        }
    }

    // -- 2xx success range --

    #[test]
    fn ok_200_passes() {
        assert!(check_response(&response(200, "")).is_ok());
    }

    #[test]
    fn created_201_passes() {
        assert!(check_response(&response(201, "")).is_ok());
    }

    #[test]
    fn no_content_204_passes() {
        assert!(check_response(&response(204, "")).is_ok());
    }

    // -- XRPC error bodies --

    #[test]
    fn error_500_with_xrpc_body() {
        let r = response(
            500,
            r#"{"error":"InternalServerError","message":"Internal Server Error"}"#,
        );
        let err = check_response(&r).unwrap_err();
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
        let err = check_response(&r).unwrap_err();
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
        let err = check_response(&r).unwrap_err();
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
        let err = check_response(&r).unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    // -- Non-JSON error bodies --

    #[test]
    fn error_502_with_html_body() {
        let r = response(502, "<html><body>Bad Gateway</body></html>");
        let err = check_response(&r).unwrap_err();
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
        let err = check_response(&r).unwrap_err();
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
        let err = check_response(&r).unwrap_err();
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
        assert!(check_response(&response(300, "")).is_err());
    }

    // -- Token refresh tests --

    fn expired_token_response() -> HttpResponse {
        HttpResponse {
            status: 400,
            body: br#"{"error":"ExpiredToken","message":"Token has expired"}"#.to_vec(),
        }
    }

    fn refresh_session_response() -> HttpResponse {
        let body = serde_json::json!({
            "did": "did:plc:test",
            "handle": "test.handle",
            "accessJwt": "fresh-access-jwt",
            "refreshJwt": "fresh-refresh-jwt",
        });
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn success_response(body: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: body.as_bytes().to_vec(),
        }
    }

    fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
        let session = Session {
            did: "did:plc:test".into(),
            handle: "test.handle".into(),
            access_jwt: "stale-access-jwt".into(),
            refresh_jwt: "valid-refresh-jwt".into(),
        };
        XrpcClient::with_session(mock, "https://pds.test".into(), session)
    }

    #[tokio::test]
    async fn refresh_on_expired_token_then_retry() {
        let mock = MockTransport::new();
        // First request: expired token
        mock.enqueue(expired_token_response());
        // Refresh succeeds
        mock.enqueue(refresh_session_response());
        // Retry succeeds
        mock.enqueue(success_response(r#"{"records":[]}"#));

        let mut client = mock_client(mock.clone());
        let page = client
            .list_records("app.opake.cloud.document", Some(100), None)
            .await
            .unwrap();

        assert!(page.records.is_empty());
        assert!(client.session_refreshed());

        let session = client.session().unwrap();
        assert_eq!(session.access_jwt, "fresh-access-jwt");
        assert_eq!(session.refresh_jwt, "fresh-refresh-jwt");

        // Verify: 3 requests — original, refresh, retry
        let reqs = mock.requests();
        assert_eq!(reqs.len(), 3);
        assert!(reqs[0].url.contains("listRecords"));
        assert!(reqs[1].url.contains("refreshSession"));
        assert!(reqs[2].url.contains("listRecords"));

        // Retry used the new token
        let retry_auth = reqs[2]
            .headers
            .iter()
            .find(|(k, _)| k == "Authorization")
            .unwrap();
        assert_eq!(retry_auth.1, "Bearer fresh-access-jwt");
    }

    #[tokio::test]
    async fn refresh_failure_propagates_error() {
        let mock = MockTransport::new();
        mock.enqueue(expired_token_response());
        // Refresh fails
        mock.enqueue(HttpResponse {
            status: 401,
            body: br#"{"error":"InvalidToken","message":"bad refresh token"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let err = client
            .list_records("app.opake.cloud.document", Some(100), None)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("session refresh failed"));
        assert!(err.to_string().contains("opake login"));
        assert!(!client.session_refreshed());
    }

    #[tokio::test]
    async fn non_expired_error_passes_through() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 500,
            body: br#"{"error":"InternalServerError","message":"oops"}"#.to_vec(),
        });

        let mut client = mock_client(mock);
        let err = client
            .list_records("app.opake.cloud.document", Some(100), None)
            .await
            .unwrap_err();

        assert!(matches!(err, Error::Xrpc { status: 500, .. }));
        assert!(!client.session_refreshed());
    }

    #[test]
    fn is_expired_token_detects_correctly() {
        assert!(XrpcClient::<MockTransport>::is_expired_token(
            &expired_token_response()
        ));
    }

    #[test]
    fn is_expired_token_rejects_other_400() {
        let r = response(400, r#"{"error":"InvalidRequest"}"#);
        assert!(!XrpcClient::<MockTransport>::is_expired_token(&r));
    }

    #[test]
    fn is_expired_token_rejects_500() {
        let r = response(500, r#"{"error":"ExpiredToken"}"#);
        assert!(!XrpcClient::<MockTransport>::is_expired_token(&r));
    }

    #[test]
    fn is_expired_token_rejects_no_json() {
        let r = response(400, "not json");
        assert!(!XrpcClient::<MockTransport>::is_expired_token(&r));
    }

    #[tokio::test]
    async fn put_record_sends_rkey_and_returns_ref() {
        let mock = MockTransport::new();
        let body = serde_json::json!({
            "uri": "at://did:plc:test/app.opake.cloud.publicKey/self",
            "cid": "bafyputrecord",
        });
        mock.enqueue(success_response(&body.to_string()));

        let mut client = mock_client(mock.clone());

        let record = serde_json::json!({ "hello": "world" });
        let result = client
            .put_record("app.opake.cloud.publicKey", "self", &record)
            .await
            .unwrap();

        assert_eq!(
            result.uri,
            "at://did:plc:test/app.opake.cloud.publicKey/self"
        );
        assert_eq!(result.cid, "bafyputrecord");

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].url.contains("putRecord"));

        // Verify the body includes rkey
        let sent_body = match &reqs[0].body {
            Some(RequestBody::Json(v)) => v.clone(),
            _ => panic!("expected JSON body"),
        };
        assert_eq!(sent_body["rkey"], "self");
        assert_eq!(sent_body["collection"], "app.opake.cloud.publicKey");
        assert_eq!(sent_body["repo"], "did:plc:test");
    }
}
