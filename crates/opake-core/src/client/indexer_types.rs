// Client-side response types for the indexer JSON API.
//
// These mirror the indexer's server-side types but only carry Deserialize —
// this crate doesn't need to serialize them.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InboxGrant {
    pub uri: String,
    pub owner_did: String,
    pub document_uri: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InboxResponse {
    pub grants: Vec<InboxGrant>,
    pub cursor: Option<String>,
}

/// A workspace document from the /api/workspace endpoint.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceDocument {
    pub document_uri: String,
    pub keyring_uri: String,
    pub owner_did: String,
    pub rotation: u64,
    pub indexed_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceResponse {
    pub documents: Vec<WorkspaceDocument>,
    pub cursor: Option<String>,
}

/// A keyring the user is a member of, from /api/keyrings.
///
/// The Indexer indexes full keyring data from the firehose so clients
/// don't need raw XRPC listRecords calls.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IndexerKeyring {
    pub uri: String,
    pub owner_did: String,
    pub rotation: u64,
    #[serde(default)]
    pub members: Vec<serde_json::Value>,
    pub encrypted_metadata: Option<serde_json::Value>,
    pub created_at: Option<String>,
    pub indexed_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeyringsResponse {
    pub keyrings: Vec<IndexerKeyring>,
    pub cursor: Option<String>,
}

// ---------------------------------------------------------------------------
// Tree sync types — delta broker responses
// ---------------------------------------------------------------------------

/// A directory record from the Indexer tree/sync endpoints.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TreeDirectory {
    pub directory_uri: String,
    pub owner_did: String,
    #[serde(default)]
    pub entries: Vec<String>,
    pub encrypted_metadata: Option<serde_json::Value>,
    pub key_wrapping: Option<serde_json::Value>,
    pub deleted_at: Option<String>,
    pub indexed_at: String,
}

/// A document record from the Indexer tree/sync endpoints.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TreeDocument {
    pub document_uri: String,
    pub owner_did: String,
    pub encrypted_metadata: Option<serde_json::Value>,
    pub encryption: Option<serde_json::Value>,
    pub blob_ref: Option<serde_json::Value>,
    pub deleted_at: Option<String>,
    pub indexed_at: String,
}

/// A pending directory update proposal from a workspace member.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TreeProposal {
    pub uri: String,
    pub author_did: String,
    pub action_type: String,
    pub directory_uri: Option<String>,
    pub entry_uri: Option<String>,
    pub encrypted_metadata: Option<serde_json::Value>,
    pub source_directory_uri: Option<String>,
    pub target_directory_uri: Option<String>,
    pub parent_directory_uri: Option<String>,
    pub indexed_at: String,
}

/// A pending keyring update proposal from a workspace member.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KeyringProposal {
    pub uri: String,
    pub author_did: String,
    pub action_type: String,
    pub member_did: Option<String>,
    pub member_public_key: Option<String>,
    pub role: Option<String>,
    pub encrypted_metadata: Option<serde_json::Value>,
    pub indexed_at: String,
}

/// A pending document update proposal from a workspace member.
/// The Indexer stores only metadata — the full record (blob ref, encrypted
/// metadata) lives on the proposer's PDS and must be fetched for processing.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DocumentProposal {
    pub uri: String,
    pub document_uri: String,
    pub author_did: String,
    #[serde(default)]
    pub supersedes_uri: Option<String>,
    pub indexed_at: String,
}

/// Response from /api/cabinet/tree, /api/cabinet/sync,
/// /api/workspace/tree, /api/workspace/sync.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TreeDelta {
    pub directories: Vec<TreeDirectory>,
    pub documents: Vec<TreeDocument>,
    #[serde(default)]
    pub proposals: Vec<TreeProposal>,
    #[serde(default, rename = "keyringProposals")]
    pub keyring_proposals: Vec<KeyringProposal>,
    #[serde(default, rename = "documentProposals")]
    pub document_proposals: Vec<DocumentProposal>,
    pub server_time: String,
}

// ---------------------------------------------------------------------------
// Cache conversions
// ---------------------------------------------------------------------------

impl TreeDirectory {
    /// Convert an Indexer directory into a `CachedRecord` for local storage.
    ///
    /// Reconstructs a PDS-compatible record JSON so `DirectoryTree::from_cached_records`
    /// can deserialize it as a `Directory`.
    pub fn to_cached_record(&self) -> crate::storage::CachedRecord {
        let value = serde_json::json!({
            "opakeVersion": 1,
            "keyWrapping": self.key_wrapping,
            "encryptedMetadata": self.encrypted_metadata,
            "entries": self.entries,
            "createdAt": self.indexed_at,
        });
        crate::storage::CachedRecord {
            uri: self.directory_uri.clone(),
            cid: String::new(),
            value,
        }
    }
}

impl TreeDocument {
    /// Convert an Indexer document into a `CachedRecord` for local storage.
    pub fn to_cached_record(&self) -> crate::storage::CachedRecord {
        let value = serde_json::json!({
            "opakeVersion": 1,
            "encryption": self.encryption,
            "encryptedMetadata": self.encrypted_metadata,
            "blob": self.blob_ref,
            "createdAt": self.indexed_at,
        });
        crate::storage::CachedRecord {
            uri: self.document_uri.clone(),
            cid: String::new(),
            value,
        }
    }
}

impl TreeDelta {
    /// Convert all directory entries into cached records.
    pub fn directory_cache_records(&self) -> Vec<crate::storage::CachedRecord> {
        self.directories
            .iter()
            .filter(|d| d.deleted_at.is_none())
            .map(|d| d.to_cached_record())
            .collect()
    }

    /// Convert all document entries into cached records.
    pub fn document_cache_records(&self) -> Vec<crate::storage::CachedRecord> {
        self.documents
            .iter()
            .filter(|d| d.deleted_at.is_none())
            .map(|d| d.to_cached_record())
            .collect()
    }

    /// Parse `server_time` as epoch milliseconds for `CachedCollection.fetched_at`.
    ///
    /// Uses current system time as the timestamp — what matters is that the
    /// next sync passes this value back as `since`, and the server handles
    /// the ISO8601 format. The millis value is just a local staleness marker.
    pub fn fetched_at_millis(&self) -> u64 {
        super::time::unix_now_millis()
    }

    /// The server timestamp to pass as `since` on the next sync request.
    pub fn sync_cursor(&self) -> &str {
        &self.server_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_response() {
        let json = r#"{
            "grants": [{
                "uri": "at://did:plc:owner/app.opake.grant/tid1",
                "owner_did": "did:plc:owner",
                "document_uri": "at://did:plc:owner/app.opake.document/doc1",
                "created_at": "2026-03-01T12:00:00Z"
            }],
            "cursor": "next-page"
        }"#;

        let resp: InboxResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.grants.len(), 1);
        assert_eq!(resp.grants[0].owner_did, "did:plc:owner");
        assert_eq!(
            resp.grants[0].document_uri,
            "at://did:plc:owner/app.opake.document/doc1"
        );
        assert_eq!(resp.cursor.as_deref(), Some("next-page"));
    }

    #[test]
    fn deserialize_empty_response() {
        let json = r#"{"grants": []}"#;
        let resp: InboxResponse = serde_json::from_str(json).unwrap();
        assert!(resp.grants.is_empty());
        assert!(resp.cursor.is_none());
    }
}
