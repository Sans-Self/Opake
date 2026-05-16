// Client-side response types for the indexer JSON API.
//
// Every record-returning endpoint emits envelope-shaped JSON:
//
//     { "record": <verbatim PDS JSON>, "indexedAt": "...", "deletedAt": "..." }
//
// `IndexerEnvelope<T>` deserializes that shape into a strongly-typed
// inner record (one of `records::Directory`/`Document`/`Keyring`/`Grant`).
// The Rust crate uses the same struct family for both the PDS read path
// and the indexer read path — there are no parallel `Sse*Record` or
// `Tree*` shadow types.

use serde::{Deserialize, Serialize};

use crate::records::{Directory, Document, Grant, Keyring};

/// Indexer envelope. Carries the verbatim on-PDS record JSON plus
/// indexer-managed metadata as siblings. `T` is the record type
/// (typically a `records::*` struct).
///
/// The `uri` field lives at the envelope level rather than inside the
/// record because atproto records don't carry their own AT-URI — the
/// URI is identity metadata, not content. Keeping it on the envelope
/// preserves the Pillar-1 invariant that `record` is byte-identical to
/// what the PDS holds.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerEnvelope<T> {
    /// The record's AT-URI.
    pub uri: String,
    /// Byte-identical to what the PDS holds (modulo JSON key ordering).
    pub record: T,
    /// Server-side timestamp at which the indexer committed the record.
    pub indexed_at: String,
    /// Soft-delete tombstone; `None` for live records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
}

impl<T> IndexerEnvelope<T> {
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

// ---------------------------------------------------------------------------
// Inbox
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct InboxResponse {
    pub grants: Vec<IndexerEnvelope<Grant>>,
    #[serde(default)]
    pub cursor: Option<String>,
}

// ---------------------------------------------------------------------------
// Workspaces (member listing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspacesResponse {
    pub workspaces: Vec<IndexerEnvelope<Keyring>>,
}

// ---------------------------------------------------------------------------
// Tree sync — snapshots and deltas
// ---------------------------------------------------------------------------

/// Response from `/cabinet/snapshot`, `/cabinet/sync`, `/workspace/snapshot`,
/// `/workspace/sync`. Directories and documents are envelopes carrying the
/// verbatim PDS record JSON.
#[derive(Debug, Clone, Deserialize)]
pub struct TreeDelta {
    pub directories: Vec<IndexerEnvelope<Directory>>,
    pub documents: Vec<IndexerEnvelope<Document>>,
    pub server_time: String,
    /// Present only on the workspace endpoint.
    #[serde(default)]
    pub workspace_id: Option<String>,
}

impl TreeDelta {
    /// The server timestamp to pass as `since` on the next sync request.
    pub fn sync_cursor(&self) -> &str {
        &self.server_time
    }

    /// Local staleness marker for the cache.
    pub fn fetched_at_millis(&self) -> u64 {
        crate::client::time::unix_now_millis()
    }
}

// ---------------------------------------------------------------------------
// Chain heads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ChainHeadResponse {
    pub head_uri: String,
    pub head_cid: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceChainHeadResponse {
    pub workspace_id: String,
    #[serde(default)]
    pub keyring: Option<ChainHeadResponse>,
    pub root_directory: Option<ChainHeadResponse>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_inbox_envelope() {
        let json = r#"{
            "grants": [
                {
                    "record": {
                        "opakeVersion": 1,
                        "document": "at://did:plc:author/app.opake.document/doc1",
                        "recipient": "did:plc:me",
                        "wrappedKey": {
                            "did": "did:plc:me",
                            "ciphertext": "AAAA",
                            "algo": "x25519-mlkem768-hkdf-a256kw-v2"
                        },
                        "encryptedMetadata": {
                            "ciphertext": "AAAA",
                            "nonce": "BBBB"
                        },
                        "createdAt": "2026-03-01T12:00:00Z"
                    },
                    "indexedAt": "2026-03-01T12:00:01Z"
                }
            ],
            "cursor": "next-page"
        }"#;

        let resp: InboxResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.grants.len(), 1);
        assert_eq!(resp.grants[0].record.recipient, "did:plc:me");
        assert_eq!(resp.cursor.as_deref(), Some("next-page"));
    }

    #[test]
    fn deserialize_empty_inbox() {
        let json = r#"{"grants": []}"#;
        let resp: InboxResponse = serde_json::from_str(json).unwrap();
        assert!(resp.grants.is_empty());
        assert!(resp.cursor.is_none());
    }
}
