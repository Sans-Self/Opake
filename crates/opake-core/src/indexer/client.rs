// Indexer client — HTTP layer for the records / chain-heads endpoints.
//
// All endpoints return envelope-shaped JSON (`{record, indexedAt, deletedAt?}`)
// for record payloads. This client deserializes envelopes once and hands
// the typed values back to the rest of the crate. No parallel `Sse*` or
// `Tree*` shadow types — `IndexerEnvelope<T>` is the single response shape.

use crate::client::{HttpMethod, HttpRequest, Transport};
use crate::directories::{ChainHead, ChainHeadProvider, WorkspaceChainHeads};
use crate::error::Error;
use crate::indexer::auth::sign_indexer_request;
use crate::indexer::types::{
    IndexerEnvelope, InboxResponse, TreeDelta, WorkspaceChainHeadResponse, WorkspacesResponse,
};
use crate::records::{Grant, Keyring};
use crate::workspace::WorkspaceId;

/// Check an indexer JSON response for errors.
fn check_indexer_response(status: u16, body: &[u8]) -> Result<(), Error> {
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

    Err(Error::Indexer { status, message })
}

/// Fetch a single page of inbox grants from the indexer.
pub async fn fetch_inbox(
    transport: &impl Transport,
    indexer_url: &str,
    did: &str,
    signing_key: &[u8; 32],
    limit: Option<u32>,
    cursor: Option<&str>,
) -> Result<InboxResponse, Error> {
    let path = "/api/inbox";
    let timestamp = crate::client::time::unix_now() as u64;
    let auth = sign_indexer_request("GET", path, did, signing_key, timestamp);

    let mut params = Vec::new();
    if let Some(l) = limit {
        params.push(format!("limit={l}"));
    }
    if let Some(c) = cursor {
        params.push(format!("cursor={c}"));
    }

    let url = if params.is_empty() {
        format!("{indexer_url}{path}")
    } else {
        format!("{indexer_url}{path}?{}", params.join("&"))
    };

    let request = HttpRequest {
        method: HttpMethod::Get,
        url,
        headers: vec![("Authorization".into(), auth)],
        body: None,
    };

    let response = transport.send(request).await?;
    check_indexer_response(response.status, &response.body)?;

    serde_json::from_slice(&response.body).map_err(|e| Error::Indexer {
        status: response.status,
        message: format!("failed to parse inbox response: {e}"),
    })
}

/// Fetch all inbox grants, paginating automatically until exhausted.
pub async fn fetch_inbox_all(
    transport: &impl Transport,
    indexer_url: &str,
    did: &str,
    signing_key: &[u8; 32],
) -> Result<Vec<IndexerEnvelope<Grant>>, Error> {
    let mut all_grants = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let page = fetch_inbox(
            transport,
            indexer_url,
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

/// Fetch every keyring head for which the caller is a current member.
///
/// `/api/keyrings` returns one envelope per workspace (the current
/// chain head); the envelope's `record` is the verbatim PDS keyring,
/// including its `members` array.
pub async fn fetch_member_workspaces(
    transport: &impl Transport,
    indexer_url: &str,
    did: &str,
    signing_key: &[u8; 32],
) -> Result<Vec<IndexerEnvelope<Keyring>>, Error> {
    let path = "/api/keyrings";
    let timestamp = crate::client::time::unix_now() as u64;
    let auth = sign_indexer_request("GET", path, did, signing_key, timestamp);

    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url: format!("{indexer_url}{path}"),
            headers: vec![("Authorization".into(), auth)],
            body: None,
        })
        .await?;

    check_indexer_response(response.status, &response.body)?;

    let parsed: WorkspacesResponse =
        serde_json::from_slice(&response.body).map_err(|e| Error::Indexer {
            status: response.status,
            message: format!("failed to parse workspaces response: {e}"),
        })?;

    Ok(parsed.workspaces)
}

// ---------------------------------------------------------------------------
// Tree sync — snapshot + delta endpoints
// ---------------------------------------------------------------------------

async fn indexer_get(
    transport: &impl Transport,
    indexer_url: &str,
    path: &str,
    did: &str,
    signing_key: &[u8; 32],
    query: &str,
) -> Result<Vec<u8>, Error> {
    let timestamp = crate::client::time::unix_now() as u64;
    let auth = sign_indexer_request("GET", path, did, signing_key, timestamp);
    let url = if query.is_empty() {
        format!("{indexer_url}{path}")
    } else {
        format!("{indexer_url}{path}?{query}")
    };
    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url,
            headers: vec![("Authorization".into(), auth)],
            body: None,
        })
        .await?;
    check_indexer_response(response.status, &response.body)?;
    Ok(response.body)
}

pub async fn fetch_cabinet_snapshot(
    transport: &impl Transport,
    indexer_url: &str,
    did: &str,
    signing_key: &[u8; 32],
) -> Result<TreeDelta, Error> {
    let body = indexer_get(
        transport,
        indexer_url,
        "/api/cabinet/snapshot",
        did,
        signing_key,
        "",
    )
    .await?;
    serde_json::from_slice(&body).map_err(|e| Error::Indexer {
        status: 200,
        message: format!("failed to parse cabinet snapshot response: {e}"),
    })
}

pub async fn fetch_cabinet_sync(
    transport: &impl Transport,
    indexer_url: &str,
    did: &str,
    signing_key: &[u8; 32],
    since: &str,
) -> Result<TreeDelta, Error> {
    let query = format!("since={since}");
    let body = indexer_get(
        transport,
        indexer_url,
        "/api/cabinet/sync",
        did,
        signing_key,
        &query,
    )
    .await?;
    serde_json::from_slice(&body).map_err(|e| Error::Indexer {
        status: 200,
        message: format!("failed to parse cabinet sync response: {e}"),
    })
}

pub async fn fetch_workspace_snapshot(
    transport: &impl Transport,
    indexer_url: &str,
    did: &str,
    signing_key: &[u8; 32],
    workspace_id: &WorkspaceId,
) -> Result<TreeDelta, Error> {
    let query = format!("workspace_id={}", workspace_id.as_str());
    let body = indexer_get(
        transport,
        indexer_url,
        "/api/workspace/snapshot",
        did,
        signing_key,
        &query,
    )
    .await?;
    serde_json::from_slice(&body).map_err(|e| Error::Indexer {
        status: 200,
        message: format!("failed to parse workspace snapshot response: {e}"),
    })
}

pub async fn fetch_workspace_sync(
    transport: &impl Transport,
    indexer_url: &str,
    did: &str,
    signing_key: &[u8; 32],
    workspace_id: &WorkspaceId,
    since: &str,
) -> Result<TreeDelta, Error> {
    let query = format!("workspace_id={}&since={since}", workspace_id.as_str());
    let body = indexer_get(
        transport,
        indexer_url,
        "/api/workspace/sync",
        did,
        signing_key,
        &query,
    )
    .await?;
    serde_json::from_slice(&body).map_err(|e| Error::Indexer {
        status: 200,
        message: format!("failed to parse workspace sync response: {e}"),
    })
}

/// Request a short-lived SSE token from the indexer.
pub async fn request_sse_token(
    transport: &impl Transport,
    indexer_url: &str,
    did: &str,
    signing_key: &[u8; 32],
) -> Result<String, Error> {
    let path = "/api/events/token";
    let timestamp = crate::client::time::unix_now() as u64;
    let auth = sign_indexer_request("POST", path, did, signing_key, timestamp);

    let request = HttpRequest {
        method: HttpMethod::Post,
        url: format!("{indexer_url}{path}"),
        headers: vec![("Authorization".into(), auth)],
        body: None,
    };

    let response = transport.send(request).await?;
    check_indexer_response(response.status, &response.body)?;

    #[derive(serde::Deserialize)]
    struct TokenResponse {
        token: String,
    }

    let parsed: TokenResponse =
        serde_json::from_slice(&response.body).map_err(|e| Error::Indexer {
            status: response.status,
            message: format!("failed to parse SSE token response: {e}"),
        })?;

    Ok(parsed.token)
}

// ---------------------------------------------------------------------------
// Chain head lookup
// ---------------------------------------------------------------------------

pub async fn fetch_workspace_chain_heads(
    transport: &impl Transport,
    indexer_url: &str,
    did: &str,
    signing_key: &[u8; 32],
    workspace_id: &WorkspaceId,
) -> Result<WorkspaceChainHeadResponse, Error> {
    let query = format!("workspace_id={}", workspace_id.as_str());
    let body = indexer_get(
        transport,
        indexer_url,
        "/api/workspace/chain-head",
        did,
        signing_key,
        &query,
    )
    .await?;
    serde_json::from_slice(&body).map_err(|e| Error::Indexer {
        status: 200,
        message: format!("failed to parse chain-head response: {e}"),
    })
}

/// `ChainHeadProvider` backed by the hosted indexer.
pub struct IndexerChainHeadProvider<'a, T: Transport> {
    pub transport: &'a T,
    pub indexer_url: &'a str,
    pub did: &'a str,
    pub signing_key: &'a [u8; 32],
}

impl<T: Transport> ChainHeadProvider for IndexerChainHeadProvider<'_, T> {
    async fn workspace_chain_heads(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<WorkspaceChainHeads, Error> {
        let response = fetch_workspace_chain_heads(
            self.transport,
            self.indexer_url,
            self.did,
            self.signing_key,
            workspace_id,
        )
        .await?;

        Ok(WorkspaceChainHeads {
            keyring: response.keyring.map(|r| ChainHead {
                uri: r.head_uri,
                cid: r.head_cid,
            }),
            root_directory: response.root_directory.map(|r| ChainHead {
                uri: r.head_uri,
                cid: r.head_cid,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::HttpResponse;
    use crate::test_utils::MockTransport;

    fn dummy_key() -> [u8; 32] {
        [42u8; 32]
    }

    fn grant_envelope_json(suffix: &str, recipient: &str) -> String {
        format!(
            r#"{{
                "uri": "at://did:plc:author/app.opake.grant/{suffix}",
                "record": {{
                    "opakeVersion": 1,
                    "document": "at://did:plc:author/app.opake.document/doc-{suffix}",
                    "recipient": "{recipient}",
                    "wrappedKey": {{
                        "did": "{recipient}",
                        "ciphertext": {{"$bytes": "AAAA"}},
                        "algo": "x25519-mlkem768-hkdf-a256kw-v2"
                    }},
                    "encryptedMetadata": {{
                        "ciphertext": {{"$bytes": "AAAA"}},
                        "nonce": {{"$bytes": "BBBB"}}
                    }},
                    "createdAt": "2026-03-01T12:00:00Z"
                }},
                "indexedAt": "2026-03-01T12:00:01Z"
            }}"#
        )
    }

    fn inbox_json(envelopes: &[String], cursor: Option<&str>) -> Vec<u8> {
        let grants_str = envelopes.join(",");
        let cursor_str = match cursor {
            Some(c) => format!(r#","cursor":"{c}""#),
            None => String::new(),
        };
        format!(r#"{{"grants":[{grants_str}]{cursor_str}}}"#).into_bytes()
    }

    #[tokio::test]
    async fn fetch_inbox_single_page() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: inbox_json(
                &[
                    grant_envelope_json("g1", "did:plc:me"),
                    grant_envelope_json("g2", "did:plc:me"),
                ],
                None,
            ),
        });

        let resp = fetch_inbox(
            &mock,
            "https://indexer.test",
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
        assert_eq!(req.url, "https://indexer.test/api/inbox");
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
            body: inbox_json(&[grant_envelope_json("g1", "did:plc:me")], Some("cursor1")),
        });
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: inbox_json(&[grant_envelope_json("g2", "did:plc:me")], None),
        });

        let grants = fetch_inbox_all(&mock, "https://indexer.test", "did:plc:me", &dummy_key())
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
            "https://indexer.test",
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
            "https://indexer.test",
            "did:plc:me",
            &dummy_key(),
            None,
            None,
        )
        .await
        .unwrap_err();

        match err {
            Error::Indexer { status, message } => {
                assert_eq!(status, 401);
                assert!(message.contains("signature verification failed"));
            }
            other => panic!("expected Indexer error, got: {other:?}"),
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
            "https://indexer.test",
            "did:plc:me",
            &dummy_key(),
            None,
            None,
        )
        .await
        .unwrap_err();

        match err {
            Error::Indexer { status, .. } => assert_eq!(status, 500),
            other => panic!("expected Indexer error, got: {other:?}"),
        }
    }
}
