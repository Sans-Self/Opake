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
    InboxResponse, IndexerEnvelope, TreeDelta, WorkspaceChainHeadResponse, WorkspacesResponse,
};
use crate::records::{Grant, Keyring};
use crate::workspace::WorkspaceId;

/// The machine-readable code a workspace-scoped endpoint returns when it holds
/// no keyring chain head for the requested workspace id. The status (404) is
/// there for proxies and logs; this code is the contract, so classification
/// branches on it rather than on the bare status integer.
pub const WORKSPACE_NOT_INDEXED: &str = "workspace_not_indexed";

/// The `error` field of an indexer error body, when it has one. Carries either
/// a machine-readable code (workspace-scoped endpoints) or prose.
fn indexer_error_field(body: &[u8]) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct ErrorBody {
        error: Option<String>,
    }

    serde_json::from_slice::<ErrorBody>(body)
        .ok()
        .and_then(|e| e.error)
}

/// Check an indexer JSON response for errors.
fn check_indexer_response(status: u16, body: &[u8]) -> Result<(), Error> {
    if (200..300).contains(&status) {
        return Ok(());
    }

    let message = indexer_error_field(body).unwrap_or_else(|| format!("HTTP {status}"));

    Err(Error::Indexer { status, message })
}

/// Check a response from a workspace-scoped endpoint (`/workspace/snapshot`,
/// `/workspace/sync`, `/workspace/chain-head`), which answers the membership
/// gate with a three-way split: `workspace_not_indexed` when no keyring chain
/// head exists for the id, 403 once a head was consulted and the caller is
/// absent from its `members[]`, and the payload otherwise.
///
/// Both failure classes get a named error so the retry boundary can absorb the
/// first (transient) and surface the second (definitive) without either side
/// re-deriving the wire contract.
fn check_workspace_response(
    status: u16,
    body: &[u8],
    workspace_id: &WorkspaceId,
) -> Result<(), Error> {
    if (200..300).contains(&status) {
        return Ok(());
    }

    if indexer_error_field(body).as_deref() == Some(WORKSPACE_NOT_INDEXED) {
        return Err(Error::WorkspaceNotIndexed {
            workspace_id: workspace_id.as_str().to_owned(),
        });
    }

    if status == 403 {
        return Err(Error::NotWorkspaceMember {
            workspace_id: workspace_id.as_str().to_owned(),
        });
    }

    check_indexer_response(status, body)
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

async fn indexer_get_raw(
    transport: &impl Transport,
    indexer_url: &str,
    path: &str,
    did: &str,
    signing_key: &[u8; 32],
    query: &str,
) -> Result<crate::client::HttpResponse, Error> {
    let timestamp = crate::client::time::unix_now() as u64;
    let auth = sign_indexer_request("GET", path, did, signing_key, timestamp);
    let url = if query.is_empty() {
        format!("{indexer_url}{path}")
    } else {
        format!("{indexer_url}{path}?{query}")
    };
    transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url,
            headers: vec![("Authorization".into(), auth)],
            body: None,
        })
        .await
}

async fn indexer_get(
    transport: &impl Transport,
    indexer_url: &str,
    path: &str,
    did: &str,
    signing_key: &[u8; 32],
    query: &str,
) -> Result<Vec<u8>, Error> {
    let response = indexer_get_raw(transport, indexer_url, path, did, signing_key, query).await?;
    check_indexer_response(response.status, &response.body)?;
    Ok(response.body)
}

/// GET a workspace-scoped endpoint, classifying its membership-gate answers
/// through [`check_workspace_response`].
async fn workspace_get(
    transport: &impl Transport,
    indexer_url: &str,
    path: &str,
    did: &str,
    signing_key: &[u8; 32],
    workspace_id: &WorkspaceId,
    query: &str,
) -> Result<Vec<u8>, Error> {
    let response = indexer_get_raw(transport, indexer_url, path, did, signing_key, query).await?;
    check_workspace_response(response.status, &response.body, workspace_id)?;
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
    let body = workspace_get(
        transport,
        indexer_url,
        "/api/workspace/snapshot",
        did,
        signing_key,
        workspace_id,
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
    let body = workspace_get(
        transport,
        indexer_url,
        "/api/workspace/sync",
        did,
        signing_key,
        workspace_id,
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
    let body = workspace_get(
        transport,
        indexer_url,
        "/api/workspace/chain-head",
        did,
        signing_key,
        workspace_id,
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
                "uri": "at://did:plc:author/at.opake.grant/{suffix}",
                "record": {{
                    "opakeVersion": 1,
                    "document": "at://did:plc:author/at.opake.document/doc-{suffix}",
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

    // spec:sharing-grants § The recipient discovers shares through the indexer, not by polling PDSes
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

    // spec:sharing-grants § The recipient discovers shares through the indexer, not by polling PDSes
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

    // spec:sharing-grants § The recipient discovers shares through the indexer, not by polling PDSes
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

    // ---------------------------------------------------------------------
    // Workspace-scoped membership gate — the 404-with-code / 403 split
    // ---------------------------------------------------------------------

    fn workspace() -> WorkspaceId {
        WorkspaceId::from_resolved("at://did:plc:me/at.opake.keyring/genesis")
    }

    async fn chain_head_error(status: u16, body: &str) -> Error {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status,
            headers: vec![],
            body: body.as_bytes().to_vec(),
        });

        fetch_workspace_chain_heads(
            &mock,
            "https://indexer.test",
            "did:plc:me",
            &dummy_key(),
            &workspace(),
        )
        .await
        .unwrap_err()
    }

    // spec:indexer-consistency § Dependent operations tolerate the visibility gap
    #[tokio::test]
    async fn workspace_not_indexed_code_is_the_transient_signal() {
        let err = chain_head_error(404, r#"{"error":"workspace_not_indexed"}"#).await;
        match err {
            Error::WorkspaceNotIndexed { workspace_id } => {
                assert_eq!(workspace_id, workspace().as_str());
            }
            other => panic!("expected WorkspaceNotIndexed, got: {other:?}"),
        }
        assert!(crate::indexer::retry::is_visibility_gap(
            &chain_head_error(404, r#"{"error":"workspace_not_indexed"}"#).await
        ));
    }

    // spec:indexer-consistency § Dependent operations tolerate the visibility gap
    #[tokio::test]
    async fn workspace_403_is_a_definitive_denial() {
        let err = chain_head_error(403, r#"{"error":"not a member of this workspace"}"#).await;
        match err {
            Error::NotWorkspaceMember { ref workspace_id } => {
                assert_eq!(workspace_id, workspace().as_str());
            }
            ref other => panic!("expected NotWorkspaceMember, got: {other:?}"),
        }
        assert!(!crate::indexer::retry::is_visibility_gap(&err));
    }

    // A 404 with no machine-readable code is not the workspace signal — it
    // keeps the plain not-found semantics individual records rely on.
    // spec:indexer-consistency § Dependent operations tolerate the visibility gap
    #[tokio::test]
    async fn plain_404_without_code_stays_a_plain_indexer_error() {
        let err = chain_head_error(404, r#"{"error":"no such route"}"#).await;
        match err {
            Error::Indexer {
                status,
                ref message,
            } => {
                assert_eq!(status, 404);
                assert_eq!(message, "no such route");
            }
            ref other => panic!("expected Indexer error, got: {other:?}"),
        }
        assert!(crate::indexer::retry::is_visibility_gap(&err));
    }
}
