// AppView client — fetches inbox grants from the appview JSON API.
//
// Uses the Transport trait for WASM compatibility. Signs each request
// with the caller's Ed25519 key via sign_appview_request.

use crate::client::appview_auth::sign_appview_request;
use crate::client::appview_types::{InboxGrant, InboxResponse};
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

    let mut query = format!("did={did}");
    if let Some(l) = limit {
        query.push_str(&format!("&limit={l}"));
    }
    if let Some(c) = cursor {
        query.push_str(&format!("&cursor={c}"));
    }

    let url = format!("{appview_url}{path}?{query}");

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
            r#"{{"uri":"at://did:plc:owner/app.opake.grant/{uri_suffix}","ownerDid":"did:plc:owner","documentUri":"at://did:plc:owner/app.opake.document/doc1","permissions":"read","note":null,"createdAt":"2026-03-01T12:00:00Z"}}"#
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
