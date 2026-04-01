// AppView client — fetches inbox grants from the appview JSON API.
//
// Uses the Transport trait for WASM compatibility. Signs each request
// with the caller's Ed25519 key via sign_appview_request.

use crate::client::appview_auth::sign_appview_request;
use crate::client::appview_types::{
    InboxGrant, InboxResponse, KeyringsResponse, TreeDelta, WorkspaceDocument, WorkspaceResponse,
};
use crate::client::transport::{HttpMethod, HttpRequest, Transport};
use crate::error::Error;

/// Check an appview JSON response for errors.
fn check_appview_response(status: u16, body: &[u8]) -> Result<(), Error> {
    if (200..300).contains(&status) {
        return Ok(());
    }

    #[derive(serde::Deserialize)]
    struct ErrorBody {
        error: Option<String>,
    }

    let message = serde_json::from_slice::<ErrorBody>(body)
        .ok()
        .and_then(|e| e.error)
        .unwrap_or_else(|| format!("HTTP {status}"));

    Err(Error::Appview { status, message })
}

/// Fetch a single page of inbox grants from the appview.
pub async fn fetch_inbox(
    transport: &impl Transport,
    appview_url: &str,
    did: &str,
    signing_key: &[u8; 32],
    limit: Option<u32>,
    cursor: Option<&str>,
) -> Result<InboxResponse, Error> {
    let path = "/api/inbox";
    let timestamp = super::time::unix_now() as u64;
    let auth = sign_appview_request("GET", path, did, signing_key, timestamp);

    let mut params = Vec::new();
    if let Some(l) = limit {
        params.push(format!("limit={l}"));
    }
    if let Some(c) = cursor {
        params.push(format!("cursor={c}"));
    }

    let url = if params.is_empty() {
        format!("{appview_url}{path}")
    } else {
        format!("{appview_url}{path}?{}", params.join("&"))
    };

    let request = HttpRequest {
        method: HttpMethod::Get,
        url,
        headers: vec![("Authorization".into(), auth)],
        body: None,
    };

    let response = transport.send(request).await?;
    check_appview_response(response.status, &response.body)?;

    serde_json::from_slice(&response.body).map_err(|e| Error::Appview {
        status: response.status,
        message: format!("failed to parse inbox response: {e}"),
    })
}

/// Fetch all inbox grants, paginating automatically until exhausted.
pub async fn fetch_inbox_all(
    transport: &impl Transport,
    appview_url: &str,
    did: &str,
    signing_key: &[u8; 32],
) -> Result<Vec<InboxGrant>, Error> {
    let mut all_grants = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let page = fetch_inbox(
            transport,
            appview_url,
            did,
            signing_key,
            Some(100),
            cursor.as_deref(),
        )
        .await?;

        let has_more = page.cursor.is_some();
        cursor = page.cursor;
        all_grants.extend(page.grants);

        if !has_more {
            break;
        }
    }

    Ok(all_grants)
}

/// Fetch all workspace documents from the AppView, paginated.
pub async fn fetch_workspace_documents(
    transport: &impl Transport,
    appview_url: &str,
    did: &str,
    signing_key: &[u8; 32],
    keyring_uri: &str,
) -> Result<Vec<WorkspaceDocument>, Error> {
    let path = "/api/workspace";
    let mut all = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let timestamp = super::time::unix_now() as u64;
        let auth = sign_appview_request("GET", path, did, signing_key, timestamp);

        let mut query = format!("keyringUri={keyring_uri}");
        if let Some(ref c) = cursor {
            query.push_str(&format!("&cursor={c}"));
        }

        let response = transport
            .send(HttpRequest {
                method: HttpMethod::Get,
                url: format!("{appview_url}{path}?{query}"),
                headers: vec![("Authorization".into(), auth)],
                body: None,
            })
            .await?;

        check_appview_response(response.status, &response.body)?;

        let page: WorkspaceResponse =
            serde_json::from_slice(&response.body).map_err(|e| Error::Appview {
                status: response.status,
                message: format!("failed to parse workspace response: {e}"),
            })?;

        let has_more = page.cursor.is_some();
        cursor = page.cursor;
        all.extend(page.documents);

        if !has_more {
            break;
        }
    }

    Ok(all)
}

/// Fetch all keyrings the user is a member of, with full record data.
pub async fn fetch_member_keyrings(
    transport: &impl Transport,
    appview_url: &str,
    did: &str,
    signing_key: &[u8; 32],
) -> Result<Vec<super::AppviewKeyring>, Error> {
    let path = "/api/keyrings";
    let mut all = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let timestamp = super::time::unix_now() as u64;
        let auth = sign_appview_request("GET", path, did, signing_key, timestamp);

        let url = match &cursor {
            Some(c) => format!("{appview_url}{path}?cursor={c}"),
            None => format!("{appview_url}{path}"),
        };

        let response = transport
            .send(HttpRequest {
                method: HttpMethod::Get,
                url,
                headers: vec![("Authorization".into(), auth)],
                body: None,
            })
            .await?;

        check_appview_response(response.status, &response.body)?;

        let page: KeyringsResponse =
            serde_json::from_slice(&response.body).map_err(|e| Error::Appview {
                status: response.status,
                message: format!("failed to parse keyrings response: {e}"),
            })?;

        let has_more = page.cursor.is_some();
        cursor = page.cursor;
        all.extend(page.keyrings);

        if !has_more {
            break;
        }
    }

    Ok(all)
}

// ---------------------------------------------------------------------------
// Tree sync — delta broker endpoints
// ---------------------------------------------------------------------------

/// Fetch an authenticated AppView JSON endpoint.
async fn appview_get(
    transport: &impl Transport,
    appview_url: &str,
    path: &str,
    did: &str,
    signing_key: &[u8; 32],
    query: &str,
) -> Result<Vec<u8>, Error> {
    let timestamp = super::time::unix_now() as u64;
    let auth = sign_appview_request("GET", path, did, signing_key, timestamp);
    let url = if query.is_empty() {
        format!("{appview_url}{path}")
    } else {
        format!("{appview_url}{path}?{query}")
    };
    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url,
            headers: vec![("Authorization".into(), auth)],
            body: None,
        })
        .await?;
    check_appview_response(response.status, &response.body)?;
    Ok(response.body)
}

/// Fetch the full cabinet snapshot (all directories + documents for the caller's DID).
pub async fn fetch_cabinet_snapshot(
    transport: &impl Transport,
    appview_url: &str,
    did: &str,
    signing_key: &[u8; 32],
) -> Result<TreeDelta, Error> {
    let body = appview_get(
        transport,
        appview_url,
        "/api/cabinet/snapshot",
        did,
        signing_key,
        "",
    )
    .await?;
    serde_json::from_slice(&body).map_err(|e| Error::Appview {
        status: 200,
        message: format!("failed to parse cabinet snapshot response: {e}"),
    })
}

/// Fetch cabinet changes since a given timestamp.
pub async fn fetch_cabinet_sync(
    transport: &impl Transport,
    appview_url: &str,
    did: &str,
    signing_key: &[u8; 32],
    since: &str,
) -> Result<TreeDelta, Error> {
    let query = format!("since={since}");
    let body = appview_get(
        transport,
        appview_url,
        "/api/cabinet/sync",
        did,
        signing_key,
        &query,
    )
    .await?;
    serde_json::from_slice(&body).map_err(|e| Error::Appview {
        status: 200,
        message: format!("failed to parse cabinet sync response: {e}"),
    })
}

/// Fetch the full workspace snapshot.
pub async fn fetch_workspace_snapshot(
    transport: &impl Transport,
    appview_url: &str,
    did: &str,
    signing_key: &[u8; 32],
    keyring_uri: &str,
) -> Result<TreeDelta, Error> {
    let query = format!("keyring={keyring_uri}");
    let body = appview_get(
        transport,
        appview_url,
        "/api/workspace/snapshot",
        did,
        signing_key,
        &query,
    )
    .await?;
    serde_json::from_slice(&body).map_err(|e| Error::Appview {
        status: 200,
        message: format!("failed to parse workspace snapshot response: {e}"),
    })
}

/// Fetch workspace changes since a given timestamp.
pub async fn fetch_workspace_sync(
    transport: &impl Transport,
    appview_url: &str,
    did: &str,
    signing_key: &[u8; 32],
    keyring_uri: &str,
    since: &str,
) -> Result<TreeDelta, Error> {
    let query = format!("keyring={keyring_uri}&since={since}");
    let body = appview_get(
        transport,
        appview_url,
        "/api/workspace/sync",
        did,
        signing_key,
        &query,
    )
    .await?;
    serde_json::from_slice(&body).map_err(|e| Error::Appview {
        status: 200,
        message: format!("failed to parse workspace sync response: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::transport::HttpResponse;
    use crate::test_utils::MockTransport;

    fn inbox_json(grants: &[&str], cursor: Option<&str>) -> Vec<u8> {
        let grants_str = grants.join(",");
        let cursor_str = match cursor {
            Some(c) => format!(r#","cursor":"{c}""#),
            None => String::new(),
        };
        format!(r#"{{"grants":[{grants_str}]{cursor_str}}}"#).into_bytes()
    }

    fn grant_json(uri_suffix: &str) -> String {
        format!(
            r#"{{"uri":"at://did:plc:owner/app.opake.grant/{uri_suffix}","owner_did":"did:plc:owner","document_uri":"at://did:plc:owner/app.opake.document/doc1","permissions":"read","note":null,"created_at":"2026-03-01T12:00:00Z"}}"#
        )
    }

    fn dummy_key() -> [u8; 32] {
        [42u8; 32]
    }

    #[tokio::test]
    async fn fetch_inbox_single_page() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: inbox_json(&[&grant_json("g1"), &grant_json("g2")], None),
        });

        let resp = fetch_inbox(
            &mock,
            "https://appview.test",
            "did:plc:me",
            &dummy_key(),
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(resp.grants.len(), 2);
        assert!(resp.cursor.is_none());

        let req = &mock.requests()[0];
        assert!(req.url.contains("/api/inbox?did=did:plc:me"));
        assert!(req
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v.starts_with("Opake-Ed25519 ")));
    }

    #[tokio::test]
    async fn fetch_inbox_all_paginates() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: inbox_json(&[&grant_json("g1")], Some("cursor1")),
        });
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: inbox_json(&[&grant_json("g2")], None),
        });

        let grants = fetch_inbox_all(&mock, "https://appview.test", "did:plc:me", &dummy_key())
            .await
            .unwrap();

        assert_eq!(grants.len(), 2);
        assert_eq!(mock.requests().len(), 2);
        assert!(mock.requests()[1].url.contains("cursor=cursor1"));
    }

    #[tokio::test]
    async fn fetch_inbox_empty() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: inbox_json(&[], None),
        });

        let resp = fetch_inbox(
            &mock,
            "https://appview.test",
            "did:plc:me",
            &dummy_key(),
            None,
            None,
        )
        .await
        .unwrap();

        assert!(resp.grants.is_empty());
    }

    #[tokio::test]
    async fn fetch_inbox_401_error() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 401,
            headers: vec![],
            body: r#"{"error":"signature verification failed"}"#.as_bytes().to_vec(),
        });

        let err = fetch_inbox(
            &mock,
            "https://appview.test",
            "did:plc:me",
            &dummy_key(),
            None,
            None,
        )
        .await
        .unwrap_err();

        match err {
            Error::Appview { status, message } => {
                assert_eq!(status, 401);
                assert!(message.contains("signature verification failed"));
            }
            other => panic!("expected Appview error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn fetch_inbox_500_error() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 500,
            headers: vec![],
            body: r#"{"error":"internal server error"}"#.as_bytes().to_vec(),
        });

        let err = fetch_inbox(
            &mock,
            "https://appview.test",
            "did:plc:me",
            &dummy_key(),
            None,
            None,
        )
        .await
        .unwrap_err();

        match err {
            Error::Appview { status, .. } => assert_eq!(status, 500),
            other => panic!("expected Appview error, got: {other:?}"),
        }
    }
}
