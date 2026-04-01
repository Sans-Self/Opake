// Storage abstraction for config, identity, and session persistence.
//
// The types live here (opake-core) because they're plain serde structs with no
// platform dependencies. The `Storage` trait defines the contract — CLI
// implements it over the filesystem, the web frontend over IndexedDB.

use std::collections::BTreeMap;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};

use crate::client::Session;
use crate::crypto::{
    CryptoRng, Ed25519SigningKey, RngCore, X25519DalekPublicKey, X25519DalekStaticSecret,
    X25519PrivateKey, X25519PublicKey,
};
use crate::error::Error;

// ---------------------------------------------------------------------------
// Cache types
// ---------------------------------------------------------------------------

/// A single PDS record preserved for local caching (uri + content hash + raw value).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedRecord {
    pub uri: String,
    pub cid: String,
    pub value: serde_json::Value,
}

/// A snapshot of an entire collection at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedCollection {
    pub records: Vec<CachedRecord>,
    /// Unix epoch milliseconds when this snapshot was fetched from the PDS.
    pub fetched_at: u64,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Ed25519 signing key: 32 raw bytes (the secret scalar).
pub type Ed25519SecretKey = [u8; 32];
/// Ed25519 verify key: 32 raw bytes (the public point).
pub type Ed25519VerifyKey = [u8; 32];

/// Persistent CLI configuration — tracks all logged-in accounts.
///
/// Device-local only. Cross-device preferences (appview URL, telemetry)
/// live in `AccountConfigRecord` on the PDS.
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub default_did: Option<String>,
    #[serde(default)]
    pub accounts: BTreeMap<String, AccountEntry>,
    /// Whether to cache PDS records locally for faster loads.
    /// Device-local toggle — each device can have its own cache policy.
    #[serde(default = "default_cache_enabled")]
    pub cache_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_did: None,
            accounts: BTreeMap::new(),
            cache_enabled: default_cache_enabled(),
        }
    }
}

fn default_cache_enabled() -> bool {
    true
}

impl Config {
    /// Add an account. Sets it as default if no default exists yet.
    pub fn add_account(&mut self, did: String, account: AccountEntry) {
        if self.default_did.is_none() {
            self.default_did = Some(did.clone());
        }
        self.accounts.insert(did, account);
    }

    /// Remove an account. Promotes the next account as default if the removed
    /// one was the current default (BTreeMap ordering = deterministic).
    pub fn remove_account(&mut self, did: &str) -> Result<(), Error> {
        if !self.accounts.contains_key(did) {
            return Err(Error::Storage(format!("no account for {did}")));
        }
        self.accounts.remove(did);
        if self.default_did.as_deref() == Some(did) {
            self.default_did = self.accounts.keys().next().cloned();
        }
        Ok(())
    }

    /// Set the default account. Validates the DID exists in accounts.
    pub fn set_default(&mut self, did: &str) -> Result<(), Error> {
        let key = self
            .accounts
            .keys()
            .find(|k| k.as_str() == did)
            .cloned()
            .ok_or_else(|| Error::Storage(format!("no account for {did}")))?;
        self.default_did = Some(key);
        Ok(())
    }
}

/// Per-account routing entry stored in the local config.
#[derive(Debug, Serialize, Deserialize)]
pub struct AccountEntry {
    pub pds_url: String,
    pub handle: String,
}

/// Account summary returned by `Opake::list_accounts`.
#[derive(Debug)]
pub struct AccountInfo {
    pub did: String,
    pub pds_url: String,
    pub handle: String,
    pub is_default: bool,
}

/// Encryption + signing keypairs, stored as base64.
///
/// Encryption + signing keypairs, stored as base64.
///
/// `#[redact]` fields are zeroized on drop automatically (via RedactedDebug).
/// The signing fields are optional for backward compat with old identity files.
#[derive(crate::RedactedDebug, Serialize, Deserialize)]
pub struct Identity {
    pub did: String,
    pub public_key: String,
    #[redact]
    pub private_key: String,
    /// Ed25519 signing secret key (base64).
    #[serde(default)]
    #[redact]
    pub signing_key: Option<String>,
    /// Ed25519 signing public/verify key (base64).
    #[serde(default)]
    pub verify_key: Option<String>,
}

impl Identity {
    pub fn public_key_bytes(&self) -> Result<X25519PublicKey, Error> {
        decode_key_bytes(&self.public_key, "public_key")
    }

    pub fn private_key_bytes(&self) -> Result<X25519PrivateKey, Error> {
        decode_key_bytes(&self.private_key, "private_key")
    }

    pub fn signing_key_bytes(&self) -> Result<Option<Ed25519SecretKey>, Error> {
        decode_optional_key_bytes(&self.signing_key, "signing_key")
    }

    pub fn verify_key_bytes(&self) -> Result<Option<Ed25519VerifyKey>, Error> {
        decode_optional_key_bytes(&self.verify_key, "verify_key")
    }

    /// Whether this identity has Ed25519 signing keys.
    pub fn has_signing_keys(&self) -> bool {
        self.signing_key.is_some() && self.verify_key.is_some()
    }

    /// Construct from raw key bytes (e.g. from WASM where keys arrive as Uint8Array).
    ///
    /// Creates an identity without signing keys — those are only needed for
    /// AppView auth, not file operations.
    pub fn from_raw_keys(did: &str, public_key: &[u8; 32], private_key: &[u8; 32]) -> Self {
        Self {
            did: did.to_string(),
            public_key: BASE64.encode(public_key),
            private_key: BASE64.encode(private_key),
            signing_key: None,
            verify_key: None,
        }
    }

    /// Generate a new identity with random X25519 + Ed25519 keypairs.
    pub fn generate(did: &str, rng: &mut (impl CryptoRng + RngCore)) -> Self {
        let private_secret = X25519DalekStaticSecret::random_from_rng(&mut *rng);
        let public_key = X25519DalekPublicKey::from(&private_secret);
        let (signing_key, verify_key) = Self::generate_signing_keypair(rng);

        Identity {
            did: did.to_string(),
            public_key: BASE64.encode(public_key.as_bytes()),
            private_key: BASE64.encode(private_secret.to_bytes()),
            signing_key: Some(signing_key),
            verify_key: Some(verify_key),
        }
    }

    /// Add Ed25519 signing keys if missing (migration for old identities).
    /// Returns `true` if keys were added, `false` if already present.
    pub fn ensure_signing_keys(&mut self, rng: &mut (impl CryptoRng + RngCore)) -> bool {
        if self.has_signing_keys() {
            return false;
        }
        let (sk, vk) = Self::generate_signing_keypair(rng);
        self.signing_key = Some(sk);
        self.verify_key = Some(vk);
        true
    }

    fn generate_signing_keypair(rng: &mut (impl CryptoRng + RngCore)) -> (String, String) {
        let signing_key = Ed25519SigningKey::generate(rng);
        let verify_key = signing_key.verifying_key();
        (
            BASE64.encode(signing_key.to_bytes()),
            BASE64.encode(verify_key.to_bytes()),
        )
    }
}

fn decode_key_bytes<const N: usize>(b64: &str, field: &str) -> Result<[u8; N], Error> {
    let bytes = BASE64
        .decode(b64)
        .map_err(|e| Error::Storage(format!("invalid base64 in identity {field}: {e}")))?;
    bytes
        .try_into()
        .map_err(|v: Vec<u8>| Error::Storage(format!("{field} is {} bytes, expected {N}", v.len())))
}

fn decode_optional_key_bytes<const N: usize>(
    value: &Option<String>,
    field: &str,
) -> Result<Option<[u8; N]>, Error> {
    match value {
        None => Ok(None),
        Some(b64) => decode_key_bytes(b64, field).map(Some),
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Make a DID safe for use as a directory/key name: `did:plc:abc` → `did_plc_abc`.
pub fn sanitize_did(did: &str) -> String {
    did.replace(':', "_")
}

/// Resolve a handle or DID string to a DID. If the input starts with `did:`,
/// it's returned as-is. Otherwise, it's looked up as a handle in the config.
pub fn resolve_handle_or_did(config: &Config, input: &str) -> Result<String, Error> {
    if input.starts_with("did:") {
        return Ok(input.to_string());
    }
    config
        .accounts
        .iter()
        .find(|(_, acc)| acc.handle == input)
        .map(|(did, _)| did.clone())
        .ok_or_else(|| Error::Storage(format!("no account with handle {input}")))
}

// ---------------------------------------------------------------------------
// Storage trait
// ---------------------------------------------------------------------------

/// Platform-agnostic persistence for config, identity, and session data.
///
/// Follows the `Transport` pattern: RPITIT, no Send bound, uses `crate::error::Error`.
/// CLI implements this over the filesystem, web over IndexedDB.
pub trait Storage {
    fn load_config(&self) -> impl std::future::Future<Output = Result<Config, Error>>;

    fn save_config(&self, config: &Config) -> impl std::future::Future<Output = Result<(), Error>>;

    fn load_identity(
        &self,
        did: &str,
    ) -> impl std::future::Future<Output = Result<Identity, Error>>;

    fn save_identity(
        &self,
        did: &str,
        identity: &Identity,
    ) -> impl std::future::Future<Output = Result<(), Error>>;

    fn load_session(&self, did: &str) -> impl std::future::Future<Output = Result<Session, Error>>;

    fn save_session(
        &self,
        did: &str,
        session: &Session,
    ) -> impl std::future::Future<Output = Result<(), Error>>;

    fn remove_account(&self, did: &str) -> impl std::future::Future<Output = Result<(), Error>>;

    // -- Cache: record-level -------------------------------------------------

    /// Look up a single cached record by URI.
    fn cache_get_record(
        &self,
        did: &str,
        collection: &str,
        uri: &str,
    ) -> impl std::future::Future<Output = Result<Option<CachedRecord>, Error>>;

    /// Upsert one or more records into the cache (does not touch collection metadata).
    fn cache_put_records(
        &self,
        did: &str,
        collection: &str,
        records: &[CachedRecord],
    ) -> impl std::future::Future<Output = Result<(), Error>>;

    /// Remove a single record from the cache.
    fn cache_remove_record(
        &self,
        did: &str,
        collection: &str,
        uri: &str,
    ) -> impl std::future::Future<Output = Result<(), Error>>;

    // -- Cache: collection-level ----------------------------------------------

    /// Return all cached records for a collection plus the timestamp of the
    /// last full fetch, or `None` if the collection has never been fully fetched.
    fn cache_get_collection(
        &self,
        did: &str,
        collection: &str,
    ) -> impl std::future::Future<Output = Result<Option<CachedCollection>, Error>>;

    /// Atomically replace all records for a collection and set `fetched_at`.
    fn cache_put_collection(
        &self,
        did: &str,
        collection: &str,
        data: &CachedCollection,
    ) -> impl std::future::Future<Output = Result<(), Error>>;

    /// Clear the `fetched_at` timestamp (records stay for offline/record-level use).
    fn cache_invalidate_collection(
        &self,
        did: &str,
        collection: &str,
    ) -> impl std::future::Future<Output = Result<(), Error>>;

    // -- Cache: account-level ------------------------------------------------

    /// Remove all cached data for an account.
    fn cache_clear(&self, did: &str) -> impl std::future::Future<Output = Result<(), Error>>;
}

// ---------------------------------------------------------------------------
// NoopStorage — used by WASM (until IndexedDb lands) and tests.
// ---------------------------------------------------------------------------

/// No-op Storage implementation. All reads fail, all writes succeed silently.
///
/// WASM uses this because JS handles session persistence externally.
/// Tests that need real storage should use the test harness in opake-cli.
pub struct NoopStorage;

impl Storage for NoopStorage {
    async fn load_config(&self) -> Result<Config, Error> {
        Err(Error::NotFound("NoopStorage".into()))
    }
    async fn save_config(&self, _config: &Config) -> Result<(), Error> {
        Ok(())
    }
    async fn load_identity(&self, _did: &str) -> Result<Identity, Error> {
        Err(Error::NotFound("NoopStorage".into()))
    }
    async fn save_identity(&self, _did: &str, _identity: &Identity) -> Result<(), Error> {
        Ok(())
    }
    async fn load_session(&self, _did: &str) -> Result<Session, Error> {
        Err(Error::NotFound("NoopStorage".into()))
    }
    async fn save_session(&self, _did: &str, _session: &Session) -> Result<(), Error> {
        Ok(())
    }
    async fn remove_account(&self, _did: &str) -> Result<(), Error> {
        Ok(())
    }
    async fn cache_get_record(
        &self,
        _did: &str,
        _collection: &str,
        _uri: &str,
    ) -> Result<Option<CachedRecord>, Error> {
        Ok(None)
    }
    async fn cache_put_records(
        &self,
        _did: &str,
        _collection: &str,
        _records: &[CachedRecord],
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn cache_remove_record(
        &self,
        _did: &str,
        _collection: &str,
        _uri: &str,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn cache_get_collection(
        &self,
        _did: &str,
        _collection: &str,
    ) -> Result<Option<CachedCollection>, Error> {
        Ok(None)
    }
    async fn cache_put_collection(
        &self,
        _did: &str,
        _collection: &str,
        _data: &CachedCollection,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn cache_invalidate_collection(
        &self,
        _did: &str,
        _collection: &str,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn cache_clear(&self, _did: &str) -> Result<(), Error> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_did_replaces_colons() {
        assert_eq!(sanitize_did("did:plc:abc123"), "did_plc_abc123");
    }

    #[test]
    fn sanitize_did_handles_did_web() {
        assert_eq!(sanitize_did("did:web:example.com"), "did_web_example.com");
    }

    #[test]
    fn resolve_handle_or_did_passes_did_through() {
        let config = Config {
            default_did: None,
            accounts: BTreeMap::new(),
            ..Default::default()
        };
        let result = resolve_handle_or_did(&config, "did:plc:someone").unwrap();
        assert_eq!(result, "did:plc:someone");
    }

    #[test]
    fn resolve_handle_or_did_looks_up_handle() {
        let mut accounts = BTreeMap::new();
        accounts.insert(
            "did:plc:alice".to_string(),
            AccountEntry {
                pds_url: "https://pds.test".into(),
                handle: "alice.test".into(),
            },
        );
        let config = Config {
            default_did: None,
            accounts,
            ..Default::default()
        };
        let result = resolve_handle_or_did(&config, "alice.test").unwrap();
        assert_eq!(result, "did:plc:alice");
    }

    #[test]
    fn resolve_handle_or_did_unknown_handle_errors() {
        let config = Config {
            default_did: None,
            accounts: BTreeMap::new(),
            ..Default::default()
        };
        let err = resolve_handle_or_did(&config, "nobody.test").unwrap_err();
        assert!(err.to_string().contains("nobody.test"));
    }

    #[test]
    fn identity_public_key_bytes_roundtrip() {
        let identity = Identity {
            did: "did:plc:test".into(),
            public_key: BASE64.encode([1u8; 32]),
            private_key: BASE64.encode([2u8; 32]),
            signing_key: Some(BASE64.encode([3u8; 32])),
            verify_key: Some(BASE64.encode([4u8; 32])),
        };
        assert_eq!(identity.public_key_bytes().unwrap(), [1u8; 32]);
        assert_eq!(identity.private_key_bytes().unwrap(), [2u8; 32]);
        assert_eq!(identity.signing_key_bytes().unwrap().unwrap(), [3u8; 32]);
        assert_eq!(identity.verify_key_bytes().unwrap().unwrap(), [4u8; 32]);
    }

    #[test]
    fn identity_rejects_bad_base64() {
        let identity = Identity {
            did: "did:plc:test".into(),
            public_key: "not!valid!base64!!!".into(),
            private_key: BASE64.encode([0u8; 32]),
            signing_key: None,
            verify_key: None,
        };
        assert!(identity.public_key_bytes().is_err());
    }

    #[test]
    fn identity_rejects_wrong_length() {
        let identity = Identity {
            did: "did:plc:test".into(),
            public_key: BASE64.encode([0u8; 16]),
            private_key: BASE64.encode([0u8; 32]),
            signing_key: None,
            verify_key: None,
        };
        let err = identity.public_key_bytes().unwrap_err().to_string();
        assert!(err.contains("16 bytes"), "expected length in error: {err}");
    }

    #[test]
    fn has_signing_keys_requires_both() {
        let mut identity = Identity {
            did: "did:plc:test".into(),
            public_key: BASE64.encode([0u8; 32]),
            private_key: BASE64.encode([0u8; 32]),
            signing_key: None,
            verify_key: None,
        };
        assert!(!identity.has_signing_keys());

        identity.signing_key = Some(BASE64.encode([0u8; 32]));
        assert!(!identity.has_signing_keys());

        identity.verify_key = Some(BASE64.encode([0u8; 32]));
        assert!(identity.has_signing_keys());
    }

    #[test]
    fn config_default_is_empty() {
        let config = Config::default();
        assert!(config.default_did.is_none());
        assert!(config.accounts.is_empty());
    }

    // -- Config mutation methods --

    fn alice_account() -> AccountEntry {
        AccountEntry {
            pds_url: "https://pds.alice".into(),
            handle: "alice.test".into(),
        }
    }

    fn bob_account() -> AccountEntry {
        AccountEntry {
            pds_url: "https://pds.bob".into(),
            handle: "bob.test".into(),
        }
    }

    #[test]
    fn add_account_sets_default_if_first() {
        let mut config = Config::default();
        config.add_account("did:plc:alice".into(), alice_account());
        assert_eq!(config.default_did.as_deref(), Some("did:plc:alice"));
        assert_eq!(config.accounts.len(), 1);
    }

    #[test]
    fn add_account_preserves_existing_default() {
        let mut config = Config::default();
        config.add_account("did:plc:alice".into(), alice_account());
        config.add_account("did:plc:bob".into(), bob_account());
        assert_eq!(config.default_did.as_deref(), Some("did:plc:alice"));
        assert_eq!(config.accounts.len(), 2);
    }

    #[test]
    fn remove_account_promotes_next_default() {
        let mut config = Config::default();
        config.add_account("did:plc:alice".into(), alice_account());
        config.add_account("did:plc:bob".into(), bob_account());
        config.remove_account("did:plc:alice").unwrap();
        assert_eq!(config.default_did.as_deref(), Some("did:plc:bob"));
        assert_eq!(config.accounts.len(), 1);
    }

    #[test]
    fn remove_account_clears_default_when_last() {
        let mut config = Config::default();
        config.add_account("did:plc:alice".into(), alice_account());
        config.remove_account("did:plc:alice").unwrap();
        assert!(config.default_did.is_none());
        assert!(config.accounts.is_empty());
    }

    #[test]
    fn remove_account_unknown_did_errors() {
        let mut config = Config::default();
        let err = config.remove_account("did:plc:nobody").unwrap_err();
        assert!(err.to_string().contains("did:plc:nobody"));
    }

    #[test]
    fn set_default_validates_did_exists() {
        let mut config = Config::default();
        config.add_account("did:plc:alice".into(), alice_account());
        config.add_account("did:plc:bob".into(), bob_account());
        config.set_default("did:plc:bob").unwrap();
        assert_eq!(config.default_did.as_deref(), Some("did:plc:bob"));
    }

    #[test]
    fn set_default_unknown_did_errors() {
        let mut config = Config::default();
        let err = config.set_default("did:plc:nobody").unwrap_err();
        assert!(err.to_string().contains("did:plc:nobody"));
    }

    // -- Identity generation --

    use crate::crypto::OsRng;

    #[test]
    fn generate_produces_valid_identity() {
        let identity = Identity::generate("did:plc:test", &mut OsRng);
        assert_eq!(identity.did, "did:plc:test");
        assert_eq!(identity.public_key_bytes().unwrap().len(), 32);
        assert_eq!(identity.private_key_bytes().unwrap().len(), 32);
    }

    #[test]
    fn generate_always_has_signing_keys() {
        let identity = Identity::generate("did:plc:test", &mut OsRng);
        assert!(identity.has_signing_keys());
        assert!(identity.signing_key_bytes().unwrap().is_some());
        assert!(identity.verify_key_bytes().unwrap().is_some());
    }

    #[test]
    fn ensure_signing_keys_adds_when_missing() {
        let mut identity = Identity {
            did: "did:plc:test".into(),
            public_key: BASE64.encode([1u8; 32]),
            private_key: BASE64.encode([2u8; 32]),
            signing_key: None,
            verify_key: None,
        };
        assert!(!identity.has_signing_keys());
        let added = identity.ensure_signing_keys(&mut OsRng);
        assert!(added);
        assert!(identity.has_signing_keys());
    }

    #[test]
    fn ensure_signing_keys_noop_when_present() {
        let mut identity = Identity::generate("did:plc:test", &mut OsRng);
        let original_sk = identity.signing_key.clone();
        let added = identity.ensure_signing_keys(&mut OsRng);
        assert!(!added);
        assert_eq!(identity.signing_key, original_sk);
    }
}
