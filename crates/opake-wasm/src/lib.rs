use std::collections::HashMap;

use opake_core::client::dpop::DpopKeyPair;
use opake_core::client::oauth_discovery::generate_pkce;
use opake_core::crypto::{
    ContentKey, DirectoryMetadata, DocumentMetadata, EncryptedPayload, GrantMetadata,
    KeyringMetadata, OsRng, X25519PrivateKey, X25519PublicKey,
};
use opake_core::directories::{DirectoryTree, EntryKind};
use opake_core::records::{Directory, WrappedKey};
use opake_core::storage::Identity;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
mod directories;
#[cfg(target_arch = "wasm32")]
mod documents;
#[cfg(target_arch = "wasm32")]
mod sharing;
#[cfg(target_arch = "wasm32")]
pub(crate) mod wasm_util;

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).ok();
}

/// ISO 8601 UTC timestamp via JS Date.
#[cfg(target_arch = "wasm32")]
pub(crate) fn now_iso() -> String {
    js_sys::Date::new_0().to_iso_string().into()
}

#[wasm_bindgen(js_name = bindingCheck)]
pub fn binding_check() -> String {
    opake_core::binding_check().to_owned()
}

/// DTO for EncryptedPayload that serializes the nonce as Vec<u8>
/// so serde-wasm-bindgen produces a proper Uint8Array instead of
/// a plain object with numeric keys (which is what [u8; 12] gives).
#[derive(Serialize)]
struct EncryptedPayloadDto {
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
}

impl From<EncryptedPayload> for EncryptedPayloadDto {
    fn from(p: EncryptedPayload) -> Self {
        Self {
            ciphertext: p.ciphertext,
            nonce: p.nonce.to_vec(),
        }
    }
}

#[wasm_bindgen(js_name = schemaVersion)]
pub fn schema_version() -> u32 {
    opake_core::records::SCHEMA_VERSION
}

#[wasm_bindgen(js_name = generateContentKey)]
pub fn generate_content_key() -> Vec<u8> {
    let key = opake_core::crypto::generate_content_key(&mut OsRng);
    key.0.to_vec()
}

#[wasm_bindgen(js_name = encryptBlob)]
pub fn encrypt_blob(key: &[u8], plaintext: &[u8]) -> Result<JsValue, JsError> {
    let content_key = content_key_from_slice(key)?;
    let payload = opake_core::crypto::encrypt_blob(&content_key, plaintext, &mut OsRng)
        .map_err(|e| JsError::new(&e.to_string()))?;
    let dto = EncryptedPayloadDto::from(payload);
    serde_wasm_bindgen::to_value(&dto).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = decryptBlob)]
pub fn decrypt_blob(key: &[u8], ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, JsError> {
    let content_key = content_key_from_slice(key)?;
    let nonce: [u8; 12] = nonce
        .try_into()
        .map_err(|_| JsError::new("nonce must be exactly 12 bytes"))?;
    let payload = EncryptedPayload {
        ciphertext: ciphertext.to_vec(),
        nonce,
    };
    opake_core::crypto::decrypt_blob(&content_key, &payload)
        .map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = wrapKey)]
pub fn wrap_key(
    content_key: &[u8],
    recipient_pub_key: &[u8],
    recipient_did: &str,
) -> Result<JsValue, JsError> {
    let content_key = content_key_from_slice(content_key)?;
    let pub_key: &X25519PublicKey = recipient_pub_key
        .try_into()
        .map_err(|_| JsError::new("recipient public key must be exactly 32 bytes"))?;
    let wrapped = opake_core::crypto::wrap_key(&content_key, pub_key, recipient_did, &mut OsRng)
        .map_err(|e| JsError::new(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&wrapped).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = unwrapKey)]
pub fn unwrap_key(wrapped_key_js: JsValue, private_key: &[u8]) -> Result<Vec<u8>, JsError> {
    let wrapped: WrappedKey =
        serde_wasm_bindgen::from_value(wrapped_key_js).map_err(|e| JsError::new(&e.to_string()))?;
    let priv_key: &X25519PrivateKey = private_key
        .try_into()
        .map_err(|_| JsError::new("private key must be exactly 32 bytes"))?;
    let content_key = opake_core::crypto::unwrap_key(&wrapped, priv_key)
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(content_key.0.to_vec())
}

#[wasm_bindgen(js_name = wrapContentKeyForKeyring)]
pub fn wrap_content_key_for_keyring(
    content_key: &[u8],
    group_key: &[u8],
) -> Result<Vec<u8>, JsError> {
    let content_key = content_key_from_slice(content_key)?;
    let group_key = content_key_from_slice(group_key)?;
    opake_core::crypto::wrap_content_key_for_keyring(&content_key, &group_key)
        .map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = unwrapContentKeyFromKeyring)]
pub fn unwrap_content_key_from_keyring(
    wrapped: &[u8],
    group_key: &[u8],
) -> Result<Vec<u8>, JsError> {
    let group_key = content_key_from_slice(group_key)?;
    let content_key = opake_core::crypto::unwrap_content_key_from_keyring(wrapped, &group_key)
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(content_key.0.to_vec())
}

fn content_key_from_slice(bytes: &[u8]) -> Result<ContentKey, JsError> {
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| JsError::new("content key must be exactly 32 bytes"))?;
    Ok(ContentKey(arr))
}

// ---------------------------------------------------------------------------
// OAuth / DPoP exports
// ---------------------------------------------------------------------------

#[wasm_bindgen(js_name = generateDpopKeyPair)]
pub fn generate_dpop_key_pair() -> Result<JsValue, JsError> {
    let keypair = DpopKeyPair::generate(&mut OsRng);
    serde_wasm_bindgen::to_value(&keypair).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = createDpopProof)]
pub fn create_dpop_proof_js(
    keypair_json: JsValue,
    method: &str,
    url: &str,
    timestamp: f64,
    nonce: Option<String>,
    access_token: Option<String>,
) -> Result<String, JsError> {
    let keypair: DpopKeyPair =
        serde_wasm_bindgen::from_value(keypair_json).map_err(|e| JsError::new(&e.to_string()))?;
    opake_core::client::dpop::create_dpop_proof(
        &keypair,
        method,
        url,
        timestamp as i64,
        nonce.as_deref(),
        access_token.as_deref(),
        &mut OsRng,
    )
    .map_err(|e| JsError::new(&e.to_string()))
}

/// DTO for PkceChallenge — the core type doesn't derive Serialize.
#[derive(Serialize)]
struct PkceChallengeDto {
    verifier: String,
    challenge: String,
}

#[wasm_bindgen(js_name = generatePkce)]
pub fn generate_pkce_js() -> Result<JsValue, JsError> {
    let pkce = generate_pkce(&mut OsRng);
    let dto = PkceChallengeDto {
        verifier: pkce.verifier,
        challenge: pkce.challenge,
    };
    serde_wasm_bindgen::to_value(&dto).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = generateIdentity)]
pub fn generate_identity_js(did: &str) -> Result<JsValue, JsError> {
    let identity = Identity::generate(did, &mut OsRng);
    serde_wasm_bindgen::to_value(&identity).map_err(|e| JsError::new(&e.to_string()))
}

// ---------------------------------------------------------------------------
// Ephemeral keypair (for device pairing)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EphemeralKeypairDto {
    public_key: Vec<u8>,
    private_key: Vec<u8>,
}

#[wasm_bindgen(js_name = generateEphemeralKeypair)]
pub fn generate_ephemeral_keypair() -> Result<JsValue, JsError> {
    let kp = opake_core::crypto::generate_ephemeral_keypair(&mut OsRng);
    let dto = EphemeralKeypairDto {
        public_key: kp.public_key.to_vec(),
        private_key: kp.private_key.to_vec(),
    };
    serde_wasm_bindgen::to_value(&dto).map_err(|e| JsError::new(&e.to_string()))
}

// ---------------------------------------------------------------------------
// Metadata encryption exports
// ---------------------------------------------------------------------------

/// Encrypt a metadata JS object with a content key.
///
/// `metadata` must be a JS object matching `DocumentMetadata`
/// (fields: name, mimeType?, size?, tags?, description?).
/// Returns a JS object with `ciphertext` (Uint8Array) and `nonce` (Uint8Array).
#[wasm_bindgen(js_name = encryptMetadata)]
pub fn encrypt_metadata_js(key: &[u8], metadata: JsValue) -> Result<JsValue, JsError> {
    let content_key = content_key_from_slice(key)?;
    let metadata: DocumentMetadata =
        serde_wasm_bindgen::from_value(metadata).map_err(|e| JsError::new(&e.to_string()))?;
    let encrypted = opake_core::crypto::encrypt_metadata(&content_key, &metadata, &mut OsRng)
        .map_err(|e| JsError::new(&e.to_string()))?;

    let ciphertext = encrypted
        .ciphertext
        .decode()
        .map_err(|e| JsError::new(&e.to_string()))?;
    let nonce = encrypted
        .nonce
        .decode()
        .map_err(|e| JsError::new(&e.to_string()))?;

    let dto = EncryptedPayloadDto { ciphertext, nonce };
    serde_wasm_bindgen::to_value(&dto).map_err(|e| JsError::new(&e.to_string()))
}

/// Decrypt encrypted metadata back to a JS object.
///
/// Returns a JS object matching `DocumentMetadata`.
#[wasm_bindgen(js_name = decryptMetadata)]
pub fn decrypt_metadata_js(
    key: &[u8],
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<JsValue, JsError> {
    let content_key = content_key_from_slice(key)?;
    let encrypted = opake_core::records::EncryptedMetadata {
        ciphertext: opake_core::records::AtBytes::from_raw(ciphertext),
        nonce: opake_core::records::AtBytes::from_raw(nonce),
    };
    let metadata: opake_core::crypto::DocumentMetadata =
        opake_core::crypto::decrypt_metadata(&content_key, &encrypted)
            .map_err(|e| JsError::new(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&metadata).map_err(|e| JsError::new(&e.to_string()))
}

// ---------------------------------------------------------------------------
// Keyring metadata encryption exports
// ---------------------------------------------------------------------------

/// Encrypt keyring metadata (name, description) with a group key.
///
/// `metadata` must be a JS object with fields: name (string), description? (string).
/// Returns `{ ciphertext: Uint8Array, nonce: Uint8Array }`.
#[wasm_bindgen(js_name = encryptKeyringMetadata)]
pub fn encrypt_keyring_metadata_js(key: &[u8], metadata: JsValue) -> Result<JsValue, JsError> {
    let group_key = content_key_from_slice(key)?;
    let metadata: KeyringMetadata =
        serde_wasm_bindgen::from_value(metadata).map_err(|e| JsError::new(&e.to_string()))?;
    let encrypted = opake_core::crypto::encrypt_metadata(&group_key, &metadata, &mut OsRng)
        .map_err(|e| JsError::new(&e.to_string()))?;

    let ciphertext = encrypted
        .ciphertext
        .decode()
        .map_err(|e| JsError::new(&e.to_string()))?;
    let nonce = encrypted
        .nonce
        .decode()
        .map_err(|e| JsError::new(&e.to_string()))?;

    let dto = EncryptedPayloadDto { ciphertext, nonce };
    serde_wasm_bindgen::to_value(&dto).map_err(|e| JsError::new(&e.to_string()))
}

/// Decrypt keyring metadata back to a JS object.
///
/// Returns `{ name: string, description?: string }`.
#[wasm_bindgen(js_name = decryptKeyringMetadata)]
pub fn decrypt_keyring_metadata_js(
    key: &[u8],
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<JsValue, JsError> {
    let group_key = content_key_from_slice(key)?;
    let encrypted = opake_core::records::EncryptedMetadata {
        ciphertext: opake_core::records::AtBytes::from_raw(ciphertext),
        nonce: opake_core::records::AtBytes::from_raw(nonce),
    };
    let metadata: KeyringMetadata = opake_core::crypto::decrypt_metadata(&group_key, &encrypted)
        .map_err(|e| JsError::new(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&metadata).map_err(|e| JsError::new(&e.to_string()))
}

// ---------------------------------------------------------------------------
// Grant metadata encryption exports
// ---------------------------------------------------------------------------

/// Encrypt grant metadata (permissions, note) with a content key.
///
/// `metadata` must be a JS object with fields: permissions? (string), note? (string).
/// Returns `{ ciphertext: Uint8Array, nonce: Uint8Array }`.
#[wasm_bindgen(js_name = encryptGrantMetadata)]
pub fn encrypt_grant_metadata_js(key: &[u8], metadata: JsValue) -> Result<JsValue, JsError> {
    let content_key = content_key_from_slice(key)?;
    let metadata: GrantMetadata =
        serde_wasm_bindgen::from_value(metadata).map_err(|e| JsError::new(&e.to_string()))?;
    let encrypted = opake_core::crypto::encrypt_metadata(&content_key, &metadata, &mut OsRng)
        .map_err(|e| JsError::new(&e.to_string()))?;

    let ciphertext = encrypted
        .ciphertext
        .decode()
        .map_err(|e| JsError::new(&e.to_string()))?;
    let nonce = encrypted
        .nonce
        .decode()
        .map_err(|e| JsError::new(&e.to_string()))?;

    let dto = EncryptedPayloadDto { ciphertext, nonce };
    serde_wasm_bindgen::to_value(&dto).map_err(|e| JsError::new(&e.to_string()))
}

/// Decrypt grant metadata back to a JS object.
///
/// Returns `{ permissions?: string, note?: string }`.
#[wasm_bindgen(js_name = decryptGrantMetadata)]
pub fn decrypt_grant_metadata_js(
    key: &[u8],
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<JsValue, JsError> {
    let content_key = content_key_from_slice(key)?;
    let encrypted = opake_core::records::EncryptedMetadata {
        ciphertext: opake_core::records::AtBytes::from_raw(ciphertext),
        nonce: opake_core::records::AtBytes::from_raw(nonce),
    };
    let metadata: GrantMetadata = opake_core::crypto::decrypt_metadata(&content_key, &encrypted)
        .map_err(|e| JsError::new(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&metadata).map_err(|e| JsError::new(&e.to_string()))
}

// ---------------------------------------------------------------------------
// Directory metadata encryption exports
// ---------------------------------------------------------------------------

/// Encrypt directory metadata (name, description) with a content key.
///
/// `metadata` must be a JS object with fields: name (string), description? (string).
/// Returns `{ ciphertext: Uint8Array, nonce: Uint8Array }`.
#[wasm_bindgen(js_name = encryptDirectoryMetadata)]
pub fn encrypt_directory_metadata_js(key: &[u8], metadata: JsValue) -> Result<JsValue, JsError> {
    let content_key = content_key_from_slice(key)?;
    let metadata: DirectoryMetadata =
        serde_wasm_bindgen::from_value(metadata).map_err(|e| JsError::new(&e.to_string()))?;
    let encrypted = opake_core::crypto::encrypt_metadata(&content_key, &metadata, &mut OsRng)
        .map_err(|e| JsError::new(&e.to_string()))?;

    let ciphertext = encrypted
        .ciphertext
        .decode()
        .map_err(|e| JsError::new(&e.to_string()))?;
    let nonce = encrypted
        .nonce
        .decode()
        .map_err(|e| JsError::new(&e.to_string()))?;

    let dto = EncryptedPayloadDto { ciphertext, nonce };
    serde_wasm_bindgen::to_value(&dto).map_err(|e| JsError::new(&e.to_string()))
}

/// Decrypt directory metadata back to a JS object.
///
/// Returns `{ name: string, description?: string }`.
#[wasm_bindgen(js_name = decryptDirectoryMetadata)]
pub fn decrypt_directory_metadata_js(
    key: &[u8],
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<JsValue, JsError> {
    let content_key = content_key_from_slice(key)?;
    let encrypted = opake_core::records::EncryptedMetadata {
        ciphertext: opake_core::records::AtBytes::from_raw(ciphertext),
        nonce: opake_core::records::AtBytes::from_raw(nonce),
    };
    let metadata: DirectoryMetadata =
        opake_core::crypto::decrypt_metadata(&content_key, &encrypted)
            .map_err(|e| JsError::new(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&metadata).map_err(|e| JsError::new(&e.to_string()))
}

// ---------------------------------------------------------------------------
// AppView auth signing
// ---------------------------------------------------------------------------

/// Sign an appview request and return the full Authorization header value.
///
/// Returns: `Opake-Ed25519 <did>:<timestamp>:<base64(signature)>`
#[wasm_bindgen(js_name = signAppviewRequest)]
pub fn sign_appview_request_js(
    method: &str,
    path: &str,
    did: &str,
    signing_key: &[u8],
    timestamp: f64,
) -> Result<String, JsError> {
    let key: [u8; 32] = signing_key
        .try_into()
        .map_err(|_| JsError::new("signing key must be exactly 32 bytes"))?;
    Ok(opake_core::client::sign_appview_request(
        method,
        path,
        did,
        &key,
        timestamp as u64,
    ))
}

// ---------------------------------------------------------------------------
// DID document utilities
// ---------------------------------------------------------------------------

/// Return the URL to fetch a DID document (PLC directory or did:web .well-known).
#[wasm_bindgen(js_name = didDocumentUrl)]
pub fn did_document_url_js(did: &str) -> Result<String, JsError> {
    opake_core::client::did_document_url(did).map_err(|e| JsError::new(&e.to_string()))
}

/// Parse a fetched DID document and extract the handle from `alsoKnownAs`.
#[wasm_bindgen(js_name = handleFromDidDocument)]
pub fn handle_from_did_document_js(doc_json: &[u8]) -> Result<Option<String>, JsError> {
    let doc: opake_core::client::DidDocument = serde_json::from_slice(doc_json)
        .map_err(|e: serde_json::Error| JsError::new(&e.to_string()))?;
    Ok(opake_core::client::handle_from_did_document(&doc))
}

/// Parse a fetched DID document and extract the PDS service endpoint.
#[wasm_bindgen(js_name = pdsFromDidDocument)]
pub fn pds_from_did_document_js(doc_json: &[u8]) -> Result<String, JsError> {
    let doc: opake_core::client::DidDocument = serde_json::from_slice(doc_json)
        .map_err(|e: serde_json::Error| JsError::new(&e.to_string()))?;
    opake_core::client::pds_from_did_document(&doc).map_err(|e| JsError::new(&e.to_string()))
}

// ---------------------------------------------------------------------------
// DirectoryTree handle (stateful WASM export)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DirectoryRecordInput {
    uri: String,
    value: Directory,
}

#[derive(Serialize)]
struct DirectorySnapshotEntry {
    name: String,
    entries: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DirectoryTreeSnapshot {
    root_uri: Option<String>,
    directories: HashMap<String, DirectorySnapshotEntry>,
}

#[derive(Serialize)]
struct DescendantCount {
    documents: usize,
    directories: usize,
}

#[derive(Serialize)]
struct DescendantEntry {
    uri: String,
    kind: String,
}

#[wasm_bindgen]
pub struct DirectoryTreeHandle {
    inner: DirectoryTree,
}

#[wasm_bindgen]
impl DirectoryTreeHandle {
    /// Build a tree from PDS directory records, decrypt all directory names.
    ///
    /// `records_js` is `Array<{ uri: string, value: DirectoryRecord }>`.
    #[wasm_bindgen(constructor)]
    pub fn new(
        records_js: JsValue,
        did: &str,
        private_key: &[u8],
    ) -> Result<DirectoryTreeHandle, JsError> {
        let inputs: Vec<DirectoryRecordInput> =
            serde_wasm_bindgen::from_value(records_js).map_err(|e| JsError::new(&e.to_string()))?;

        let records = inputs.into_iter().map(|r| (r.uri, r.value));
        let mut tree = DirectoryTree::from_records(records);

        let priv_key: &X25519PrivateKey = private_key
            .try_into()
            .map_err(|_| JsError::new("private key must be exactly 32 bytes"))?;
        tree.decrypt_names(did, priv_key);

        Ok(Self { inner: tree })
    }

    /// Bulk-transfer the entire tree state to JS as a single object.
    #[wasm_bindgen(js_name = snapshot)]
    pub fn snapshot(&self) -> Result<JsValue, JsError> {
        let mut directories = HashMap::new();
        for uri in self.inner.all_directory_uris() {
            let name = self.inner.directory_name(uri).unwrap_or("?").to_owned();
            let entries = self
                .inner
                .entries_for(uri)
                .map(|e| e.to_vec())
                .unwrap_or_default();
            directories.insert(uri.to_owned(), DirectorySnapshotEntry { name, entries });
        }

        let snap = DirectoryTreeSnapshot {
            root_uri: self.inner.root_uri().map(str::to_owned),
            directories,
        };
        let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
        snap.serialize(&serializer)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    #[wasm_bindgen(js_name = rootUri)]
    pub fn root_uri(&self) -> Option<String> {
        self.inner.root_uri().map(str::to_owned)
    }

    #[wasm_bindgen(js_name = entriesFor)]
    pub fn entries_for(&self, uri: &str) -> JsValue {
        match self.inner.entries_for(uri) {
            Some(entries) => serde_wasm_bindgen::to_value(entries).unwrap_or(JsValue::NULL),
            None => JsValue::NULL,
        }
    }

    #[wasm_bindgen(js_name = directoryName)]
    pub fn directory_name(&self, uri: &str) -> Option<String> {
        self.inner.directory_name(uri).map(str::to_owned)
    }

    #[wasm_bindgen(js_name = isDirectory)]
    pub fn is_directory(&self, uri: &str) -> bool {
        self.inner.is_directory(uri)
    }

    #[wasm_bindgen(js_name = findParent)]
    pub fn find_parent(&self, uri: &str) -> Option<String> {
        self.inner.find_parent(uri)
    }

    #[wasm_bindgen(js_name = countDescendants)]
    pub fn count_descendants(&self, uri: &str) -> JsValue {
        let (documents, directories) = self.inner.count_descendants(uri);
        serde_wasm_bindgen::to_value(&DescendantCount {
            documents,
            directories,
        })
        .unwrap_or(JsValue::NULL)
    }

    #[wasm_bindgen(js_name = collectDescendants)]
    pub fn collect_descendants(&self, uri: &str) -> JsValue {
        let descendants: Vec<DescendantEntry> = self
            .inner
            .collect_descendants(uri)
            .into_iter()
            .map(|(uri, kind)| DescendantEntry {
                uri,
                kind: match kind {
                    EntryKind::Document => "document".into(),
                    EntryKind::Directory => "directory".into(),
                },
            })
            .collect();
        serde_wasm_bindgen::to_value(&descendants).unwrap_or(JsValue::NULL)
    }
}

// ---------------------------------------------------------------------------
// Account config exports
// ---------------------------------------------------------------------------

#[wasm_bindgen(js_name = accountConfigCollection)]
pub fn account_config_collection() -> String {
    opake_core::records::ACCOUNT_CONFIG_COLLECTION.to_owned()
}

#[wasm_bindgen(js_name = accountConfigRkey)]
pub fn account_config_rkey() -> String {
    opake_core::records::ACCOUNT_CONFIG_RKEY.to_owned()
}

/// Create a default AccountConfigRecord (telemetry disabled).
///
/// Returns `{ opakeVersion, telemetryEnabled, modifiedAt }`.
#[wasm_bindgen(js_name = newAccountConfig)]
pub fn new_account_config(modified_at: &str) -> Result<JsValue, JsError> {
    let record = opake_core::records::AccountConfigRecord::new(modified_at);
    serde_wasm_bindgen::to_value(&record).map_err(|e| JsError::new(&e.to_string()))
}

// ---------------------------------------------------------------------------
// Mnemonic / seed phrase exports
// ---------------------------------------------------------------------------

/// Generate a new 24-word BIP-39 mnemonic phrase.
///
/// Returns the phrase as a space-separated string.
#[wasm_bindgen(js_name = generateMnemonic)]
pub fn generate_mnemonic_js() -> String {
    opake_core::crypto::generate_mnemonic(&mut OsRng).to_string()
}

/// Validate a BIP-39 mnemonic phrase.
///
/// Returns `true` if the phrase is valid (24 words, all in wordlist,
/// valid checksum), `false` otherwise.
#[wasm_bindgen(js_name = validateMnemonic)]
pub fn validate_mnemonic_js(phrase: &str) -> bool {
    opake_core::crypto::parse_mnemonic(phrase).is_ok()
}

/// Derive a deterministic Identity from a BIP-39 mnemonic phrase and DID.
///
/// Returns a JS object with did, publicKey, privateKey, signingKey, verifyKey
/// (all base64-encoded). Throws if the mnemonic is invalid.
#[wasm_bindgen(js_name = deriveIdentityFromMnemonic)]
pub fn derive_identity_from_mnemonic_js(phrase: &str, did: &str) -> Result<JsValue, JsError> {
    let mnemonic =
        opake_core::crypto::parse_mnemonic(phrase).map_err(|e| JsError::new(&e.to_string()))?;
    let identity = opake_core::crypto::derive_identity_from_mnemonic(&mnemonic, did);
    serde_wasm_bindgen::to_value(&identity).map_err(|e| JsError::new(&e.to_string()))
}
