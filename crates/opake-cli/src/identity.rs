use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use log::info;
use opake_core::crypto::{
    CryptoRng, Ed25519SigningKey, RngCore, X25519DalekPublicKey, X25519DalekStaticSecret,
    X25519PrivateKey, X25519PublicKey,
};
use serde::{Deserialize, Serialize};

use crate::config;

/// Ed25519 signing key: 32 raw bytes (the secret scalar).
pub type Ed25519SecretKey = [u8; 32];
/// Ed25519 verify key: 32 raw bytes (the public point).
pub type Ed25519VerifyKey = [u8; 32];

/// Encryption + signing keypairs, stored as base64 in `identity.json`.
/// The signing fields are optional for backward compat with old identity files.
#[derive(Debug, Serialize, Deserialize)]
pub struct Identity {
    pub did: String,
    pub public_key: String,
    pub private_key: String,
    /// Ed25519 signing secret key (base64).
    #[serde(default)]
    pub signing_key: Option<String>,
    /// Ed25519 signing public/verify key (base64).
    #[serde(default)]
    pub verify_key: Option<String>,
}

impl Identity {
    pub fn public_key_bytes(&self) -> anyhow::Result<X25519PublicKey> {
        let bytes = BASE64
            .decode(&self.public_key)
            .context("invalid base64 in identity public_key")?;
        let key: X25519PublicKey = bytes.try_into().map_err(|v: Vec<u8>| {
            anyhow::anyhow!("public key is {} bytes, expected 32", v.len())
        })?;
        Ok(key)
    }

    pub fn private_key_bytes(&self) -> anyhow::Result<X25519PrivateKey> {
        let bytes = BASE64
            .decode(&self.private_key)
            .context("invalid base64 in identity private_key")?;
        let key: X25519PrivateKey = bytes.try_into().map_err(|v: Vec<u8>| {
            anyhow::anyhow!("private key is {} bytes, expected 32", v.len())
        })?;
        Ok(key)
    }

    pub fn signing_key_bytes(&self) -> anyhow::Result<Option<Ed25519SecretKey>> {
        match &self.signing_key {
            None => Ok(None),
            Some(b64) => {
                let bytes = BASE64
                    .decode(b64)
                    .context("invalid base64 in identity signing_key")?;
                let key: Ed25519SecretKey = bytes.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!("signing key is {} bytes, expected 32", v.len())
                })?;
                Ok(Some(key))
            }
        }
    }

    pub fn verify_key_bytes(&self) -> anyhow::Result<Option<Ed25519VerifyKey>> {
        match &self.verify_key {
            None => Ok(None),
            Some(b64) => {
                let bytes = BASE64
                    .decode(b64)
                    .context("invalid base64 in identity verify_key")?;
                let key: Ed25519VerifyKey = bytes.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!("verify key is {} bytes, expected 32", v.len())
                })?;
                Ok(Some(key))
            }
        }
    }

    /// Whether this identity has Ed25519 signing keys.
    pub fn has_signing_keys(&self) -> bool {
        self.signing_key.is_some() && self.verify_key.is_some()
    }
}

pub fn save_identity(did: &str, identity: &Identity) -> anyhow::Result<()> {
    config::save_account_json(did, "identity.json", identity)
}

pub fn load_identity(did: &str) -> anyhow::Result<Identity> {
    config::load_account_json(did, "identity.json")
}

/// Generate a fresh Ed25519 signing keypair, returning (secret_b64, verify_b64).
fn generate_signing_keypair(rng: &mut (impl CryptoRng + RngCore)) -> (String, String) {
    let signing_key = Ed25519SigningKey::generate(rng);
    let verify_key = signing_key.verifying_key();
    (
        BASE64.encode(signing_key.to_bytes()),
        BASE64.encode(verify_key.to_bytes()),
    )
}

/// Return the existing identity if present, otherwise generate a new
/// keypair set, save it, and return it. The boolean indicates whether
/// a new keypair was generated (or an existing one was migrated).
///
/// Migration: old identity files without signing keys get Ed25519 keys
/// added transparently on load.
pub fn ensure_identity(
    did: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> anyhow::Result<(Identity, bool)> {
    if let Ok(mut existing) = load_identity(did) {
        if existing.did == did {
            if !existing.has_signing_keys() {
                info!("migrating identity: adding Ed25519 signing keypair");
                let (sk, vk) = generate_signing_keypair(rng);
                existing.signing_key = Some(sk);
                existing.verify_key = Some(vk);
                save_identity(did, &existing)?;
                return Ok((existing, true));
            }
            return Ok((existing, false));
        }
        info!(
            "identity DID mismatch (stored {}, logged in as {}) — generating new keypair",
            existing.did, did
        );
    }

    let private_secret = X25519DalekStaticSecret::random_from_rng(&mut *rng);
    let public_key = X25519DalekPublicKey::from(&private_secret);
    let (signing_key, verify_key) = generate_signing_keypair(rng);

    let identity = Identity {
        did: did.to_string(),
        public_key: BASE64.encode(public_key.as_bytes()),
        private_key: BASE64.encode(private_secret.to_bytes()),
        signing_key: Some(signing_key),
        verify_key: Some(verify_key),
    };
    save_identity(did, &identity)?;
    Ok((identity, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_harness::with_test_dir;
    use opake_core::crypto::OsRng;
    use std::collections::BTreeMap;

    fn setup_account(did: &str) {
        let mut accounts = BTreeMap::new();
        accounts.insert(
            did.to_string(),
            config::AccountConfig {
                pds_url: "https://pds.test".into(),
                handle: "test.handle".into(),
            },
        );
        config::save_config(&config::Config {
            default_did: Some(did.to_string()),
            accounts,
            appview_url: None,
        })
        .unwrap();
    }

    #[test]
    fn save_and_load_identity_roundtrip() {
        with_test_dir(|_| {
            let did = "did:plc:test";
            setup_account(did);
            let identity = Identity {
                did: did.into(),
                public_key: BASE64.encode([1u8; 32]),
                private_key: BASE64.encode([2u8; 32]),
                signing_key: Some(BASE64.encode([3u8; 32])),
                verify_key: Some(BASE64.encode([4u8; 32])),
            };
            save_identity(did, &identity).unwrap();

            let loaded = load_identity(did).unwrap();
            assert_eq!(loaded.did, identity.did);
            assert_eq!(loaded.public_key, identity.public_key);
            assert_eq!(loaded.private_key, identity.private_key);
            assert_eq!(loaded.signing_key, identity.signing_key);
            assert_eq!(loaded.verify_key, identity.verify_key);

            assert_eq!(loaded.public_key_bytes().unwrap(), [1u8; 32]);
            assert_eq!(loaded.private_key_bytes().unwrap(), [2u8; 32]);
            assert_eq!(loaded.signing_key_bytes().unwrap().unwrap(), [3u8; 32]);
            assert_eq!(loaded.verify_key_bytes().unwrap().unwrap(), [4u8; 32]);
        });
    }

    #[test]
    fn ensure_identity_generates_when_missing() {
        with_test_dir(|_| {
            let did = "did:plc:new";
            setup_account(did);
            let (identity, generated) = ensure_identity(did, &mut OsRng).unwrap();
            assert!(generated);
            assert_eq!(identity.did, did);
            assert_eq!(identity.public_key_bytes().unwrap().len(), 32);
            assert_eq!(identity.private_key_bytes().unwrap().len(), 32);
            assert!(identity.has_signing_keys());
            assert!(identity.signing_key_bytes().unwrap().is_some());
            assert!(identity.verify_key_bytes().unwrap().is_some());
        });
    }

    #[test]
    fn ensure_identity_returns_existing_when_did_matches() {
        with_test_dir(|_| {
            let did = "did:plc:same";
            setup_account(did);
            let (first, generated) = ensure_identity(did, &mut OsRng).unwrap();
            assert!(generated);

            let (second, generated) = ensure_identity(did, &mut OsRng).unwrap();
            assert!(!generated);
            assert_eq!(first.public_key, second.public_key);
            assert_eq!(first.private_key, second.private_key);
            assert_eq!(first.signing_key, second.signing_key);
        });
    }

    #[test]
    fn ensure_identity_migrates_old_identity_without_signing_keys() {
        with_test_dir(|_| {
            let did = "did:plc:legacy";
            setup_account(did);

            // Write an old-format identity (no signing keys)
            let old_identity = serde_json::json!({
                "did": did,
                "public_key": BASE64.encode([1u8; 32]),
                "private_key": BASE64.encode([2u8; 32]),
            });
            config::ensure_account_dir(did).unwrap();
            std::fs::write(
                config::account_dir(did).join("identity.json"),
                serde_json::to_string_pretty(&old_identity).unwrap(),
            )
            .unwrap();

            let (identity, generated) = ensure_identity(did, &mut OsRng).unwrap();
            assert!(generated, "migration should report as generated");
            assert!(identity.has_signing_keys());
            // X25519 keys should be preserved
            assert_eq!(identity.public_key_bytes().unwrap(), [1u8; 32]);
            assert_eq!(identity.private_key_bytes().unwrap(), [2u8; 32]);

            // Re-load should have signing keys persisted
            let reloaded = load_identity(did).unwrap();
            assert!(reloaded.has_signing_keys());
            assert_eq!(reloaded.signing_key, identity.signing_key);
        });
    }

    #[test]
    fn load_identity_rejects_garbage_json() {
        with_test_dir(|_| {
            let did = "did:plc:test";
            setup_account(did);
            config::ensure_account_dir(did).unwrap();
            std::fs::write(
                config::account_dir(did).join("identity.json"),
                "not json {{{",
            )
            .unwrap();
            assert!(load_identity(did).is_err());
        });
    }

    #[test]
    fn load_identity_rejects_valid_json_wrong_schema() {
        with_test_dir(|_| {
            let did = "did:plc:test";
            setup_account(did);
            config::ensure_account_dir(did).unwrap();
            std::fs::write(
                config::account_dir(did).join("identity.json"),
                r#"{"color": "blue"}"#,
            )
            .unwrap();
            assert!(load_identity(did).is_err());
        });
    }

    #[test]
    fn public_key_bytes_rejects_bad_base64() {
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
    fn private_key_bytes_rejects_bad_base64() {
        let identity = Identity {
            did: "did:plc:test".into(),
            public_key: BASE64.encode([0u8; 32]),
            private_key: "~~~garbage~~~".into(),
            signing_key: None,
            verify_key: None,
        };
        assert!(identity.private_key_bytes().is_err());
    }

    #[test]
    fn public_key_bytes_rejects_wrong_length() {
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
    fn private_key_bytes_rejects_wrong_length() {
        let identity = Identity {
            did: "did:plc:test".into(),
            public_key: BASE64.encode([0u8; 32]),
            private_key: BASE64.encode([0u8; 64]),
            signing_key: None,
            verify_key: None,
        };
        let err = identity.private_key_bytes().unwrap_err().to_string();
        assert!(err.contains("64 bytes"), "expected length in error: {err}");
    }

    #[test]
    fn signing_key_bytes_rejects_bad_base64() {
        let identity = Identity {
            did: "did:plc:test".into(),
            public_key: BASE64.encode([0u8; 32]),
            private_key: BASE64.encode([0u8; 32]),
            signing_key: Some("!!!bad!!!".into()),
            verify_key: None,
        };
        assert!(identity.signing_key_bytes().is_err());
    }

    #[test]
    fn verify_key_bytes_rejects_wrong_length() {
        let identity = Identity {
            did: "did:plc:test".into(),
            public_key: BASE64.encode([0u8; 32]),
            private_key: BASE64.encode([0u8; 32]),
            signing_key: None,
            verify_key: Some(BASE64.encode([0u8; 16])),
        };
        let err = identity.verify_key_bytes().unwrap_err().to_string();
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
}
