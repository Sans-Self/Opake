use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use log::info;
use opake_core::crypto::{
    CryptoRng, RngCore, X25519DalekPublicKey, X25519DalekStaticSecret, X25519PrivateKey,
    X25519PublicKey,
};
use serde::{Deserialize, Serialize};

use crate::config;

/// X25519 encryption keypair, stored as base64 in `identity.json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Identity {
    pub did: String,
    pub public_key: String,
    pub private_key: String,
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
}

pub fn save_identity(did: &str, identity: &Identity) -> anyhow::Result<()> {
    config::save_account_json(did, "identity.json", identity)
}

pub fn load_identity(did: &str) -> anyhow::Result<Identity> {
    config::load_account_json(did, "identity.json")
}

/// Return the existing identity if present, otherwise generate a new
/// X25519 keypair, save it, and return it. The boolean indicates whether
/// a new keypair was generated.
pub fn ensure_identity(
    did: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> anyhow::Result<(Identity, bool)> {
    if let Ok(existing) = load_identity(did) {
        if existing.did == did {
            return Ok((existing, false));
        }
        info!(
            "identity DID mismatch (stored {}, logged in as {}) — generating new keypair",
            existing.did, did
        );
    }

    let private_secret = X25519DalekStaticSecret::random_from_rng(&mut *rng);
    let public_key = X25519DalekPublicKey::from(&private_secret);

    let identity = Identity {
        did: did.to_string(),
        public_key: BASE64.encode(public_key.as_bytes()),
        private_key: BASE64.encode(private_secret.to_bytes()),
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
            };
            save_identity(did, &identity).unwrap();

            let loaded = load_identity(did).unwrap();
            assert_eq!(loaded.did, identity.did);
            assert_eq!(loaded.public_key, identity.public_key);
            assert_eq!(loaded.private_key, identity.private_key);

            assert_eq!(loaded.public_key_bytes().unwrap(), [1u8; 32]);
            assert_eq!(loaded.private_key_bytes().unwrap(), [2u8; 32]);
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
        };
        assert!(identity.public_key_bytes().is_err());
    }

    #[test]
    fn private_key_bytes_rejects_bad_base64() {
        let identity = Identity {
            did: "did:plc:test".into(),
            public_key: BASE64.encode([0u8; 32]),
            private_key: "~~~garbage~~~".into(),
        };
        assert!(identity.private_key_bytes().is_err());
    }

    #[test]
    fn public_key_bytes_rejects_wrong_length() {
        let identity = Identity {
            did: "did:plc:test".into(),
            public_key: BASE64.encode([0u8; 16]),
            private_key: BASE64.encode([0u8; 32]),
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
        };
        let err = identity.private_key_bytes().unwrap_err().to_string();
        assert!(err.contains("64 bytes"), "expected length in error: {err}");
    }
}
