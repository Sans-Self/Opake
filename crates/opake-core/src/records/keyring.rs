use serde::{Deserialize, Serialize};

use super::{EncryptedMetadata, KeyringMember, SCHEMA_VERSION};

/// A snapshot of a keyring's members at a given rotation, preserved so that
/// remaining members can still decrypt documents uploaded under older group keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyHistoryEntry {
    pub rotation: u64,
    pub members: Vec<KeyringMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyring {
    pub opake_version: u32,
    pub algo: String,
    pub members: Vec<KeyringMember>,
    #[serde(default)]
    pub rotation: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub key_history: Vec<KeyHistoryEntry>,
    pub encrypted_metadata: EncryptedMetadata,
    /// AT-URI of the prior canonical keyring this record supersedes, if any.
    /// Absent on the genesis keyring of a workspace. Indexers walk this
    /// back-edge to verify the chain and check that the supersede was
    /// authored by a manager of the prior keyring.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    /// This chain's genesis URI — the stable workspace identity across
    /// rotations and membership changes. Absent on the genesis keyring,
    /// which identifies itself. Present, and never changing, on every
    /// supersede so readers can resolve workspace identity without walking
    /// the chain back.
    // spec: lineage § Lineage is the chain's genesis URI, carried on every supersede
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

impl Keyring {
    /// New genesis keyring. No supersedes, no lineage — the record's own
    /// URI becomes the workspace identity.
    pub fn new(
        members: Vec<KeyringMember>,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            algo: "aes-256-gcm".into(),
            members,
            rotation: 0,
            key_history: Vec::new(),
            encrypted_metadata,
            supersedes: None,
            lineage: None,
            created_at,
            modified_at: None,
        }
    }

    /// Stamp the chain's genesis URI onto a supersede record. Genesis
    /// records leave `lineage` absent — their own URI is the identity.
    pub fn with_lineage(mut self, lineage: impl Into<String>) -> Self {
        self.lineage = Some(lineage.into());
        self
    }

    /// The lineage anchor: the chain's genesis URI, which member and
    /// `keyHistory` wraps and the metadata AAD are bound to.
    ///
    /// Group-key wraps and the keyring's metadata ciphertext are anchored to
    /// the workspace's *stable* identity — the genesis keyring URI — so they
    /// survive supersedes without re-wrapping (a manager appends a new
    /// member's wrap and carries the rest forward verbatim). That stable URI
    /// is this record's declared `lineage` once it has superseded, or the
    /// record's own URI on the genesis keyring, which identifies itself.
    ///
    /// Every site that unwraps a member key or decrypts keyring metadata
    /// must use this as its context, regardless of which URI it fetched the
    /// record at. Passing the head URI — the natural mistake, since that's
    /// what callers hold — produces an AEAD context mismatch the moment a
    /// workspace supersedes. This is the single place that resolution lives.
    // spec: workspace-identity § Genesis URI is the workspace identity
    pub fn lineage_anchor<'a>(&'a self, self_uri: &'a str) -> &'a str {
        self.lineage.as_deref().unwrap_or(self_uri)
    }
}
