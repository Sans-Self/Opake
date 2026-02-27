// Typed representations of the app.opake.cloud.* lexicon records.
//
// These mirror the lexicon JSON schemas and handle atproto's serialization
// conventions ($type discriminators, $bytes for binary data, $link for CIDs).

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// AT Protocol primitives
// ---------------------------------------------------------------------------

/// Binary data in atproto JSON: `{ "$bytes": "<base64>" }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtBytes {
    #[serde(rename = "$bytes")]
    pub encoded: String,
}

/// CID link reference: `{ "$link": "<cid>" }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CidLink {
    #[serde(rename = "$link")]
    pub cid: String,
}

/// Blob reference as returned by `com.atproto.repo.uploadBlob`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobRef {
    #[serde(rename = "$type")]
    pub blob_type: String,
    #[serde(rename = "ref")]
    pub reference: CidLink,
    pub mime_type: String,
    pub size: u64,
}

// ---------------------------------------------------------------------------
// app.opake.cloud.defs
// ---------------------------------------------------------------------------

/// A symmetric key encrypted (wrapped) to a specific DID's public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedKey {
    pub did: String,
    pub ciphertext: AtBytes,
    pub algo: String,
}

/// Describes how a blob's content was symmetrically encrypted, plus one or
/// more wrapped copies of the content key for authorized DIDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionEnvelope {
    pub algo: String,
    pub nonce: AtBytes,
    pub keys: Vec<WrappedKey>,
}

/// Reference to a keyring whose group key protects the content key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyringRef {
    pub keyring: String,
    pub wrapped_content_key: AtBytes,
    pub rotation: u64,
}

// ---------------------------------------------------------------------------
// app.opake.cloud.document — encryption union
// ---------------------------------------------------------------------------

/// Content key wrapped directly to individual DIDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectEncryption {
    pub envelope: EncryptionEnvelope,
}

/// Content key wrapped under a keyring's group key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyringEncryption {
    pub keyring_ref: KeyringRef,
    pub algo: String,
    pub nonce: AtBytes,
}

/// How to decrypt the blob — discriminated by `$type`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "$type")]
pub enum Encryption {
    #[serde(rename = "app.opake.cloud.document#directEncryption")]
    Direct(DirectEncryption),
    #[serde(rename = "app.opake.cloud.document#keyringEncryption")]
    Keyring(KeyringEncryption),
}

// ---------------------------------------------------------------------------
// app.opake.cloud.document
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub blob: BlobRef,
    pub encryption: Encryption,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

// ---------------------------------------------------------------------------
// app.opake.cloud.grant
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub document: String,
    pub recipient: String,
    pub wrapped_key: WrappedKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// app.opake.cloud.keyring
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyring {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub algo: String,
    pub members: Vec<WrappedKey>,
    #[serde(default)]
    pub rotation: u64,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}
