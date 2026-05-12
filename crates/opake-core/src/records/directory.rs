use serde::{Deserialize, Serialize};

use super::{default_version, CidLink, EncryptedMetadata, KeyWrapping, SCHEMA_VERSION};

/// One entry in a directory's listing.
///
/// The CID pins the version of the target record observed at the moment this
/// directory was written. For child directories it points at the head of that
/// path's chain at write time; for documents it points at the document record
/// itself. Indexers use these CIDs to detect concurrent supersedes that touch
/// the same child path without re-fetching every target.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingEntry {
    pub target: String,
    pub target_cid: CidLink,
}

impl ListingEntry {
    pub fn new(target: impl Into<String>, target_cid: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            target_cid: CidLink {
                cid: target_cid.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    pub key_wrapping: KeyWrapping,
    pub encrypted_metadata: EncryptedMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<ListingEntry>,
    /// AT-URI of the prior canonical directory at this path, if any. Absent
    /// on the genesis record of a chain. Indexers walk this back-edge to
    /// verify the chain and detect forks (two records superseding the same
    /// prior URI).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

impl Directory {
    pub fn new(
        key_wrapping: KeyWrapping,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            key_wrapping,
            encrypted_metadata,
            entries: Vec::new(),
            supersedes: None,
            created_at,
            modified_at: None,
        }
    }
}
