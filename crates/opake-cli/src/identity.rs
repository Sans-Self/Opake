use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use log::info;
use opake_core::crypto::{
    CryptoRng, RngCore, X25519DalekPublicKey, X25519DalekStaticSecret, X25519PrivateKey,
    X25519PublicKey,
};
use serde::{Deserialize, Serialize};

use crate::config;

const FILENAME: &str = "identity.json";

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

    #[allow(dead_code)] // used by download (#6)
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

pub fn save_identity(identity: &Identity) -> anyhow::Result<()> {
    config::save_json(FILENAME, identity)
}

pub fn load_identity() -> anyhow::Result<Identity> {
    config::load_json(FILENAME)
}

/// Return the existing identity if it matches `did`, otherwise generate a new
/// X25519 keypair, save it, and return it. The boolean indicates whether a new
/// keypair was generated.
pub fn ensure_identity(
    did: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> anyhow::Result<(Identity, bool)> {
    if let Ok(existing) = load_identity() {
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
    save_identity(&identity)?;
    Ok((identity, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_harness::with_test_dir;
    use opake_core::crypto::OsRng;
    use std::fs;

    fn file_path() -> std::path::PathBuf {
        config::data_dir().join(FILENAME)
    }

    #[test]
    fn save_and_load_identity_roundtrip() {
        with_test_dir(|_| {
            let identity = Identity {
                did: "did:plc:test".into(),
                public_key: BASE64.encode([1u8; 32]),
                private_key: BASE64.encode([2u8; 32]),
            };
            save_identity(&identity).unwrap();

            let loaded = load_identity().unwrap();
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
            let (identity, generated) = ensure_identity("did:plc:new", &mut OsRng).unwrap();
            assert!(generated);
            assert_eq!(identity.did, "did:plc:new");
            assert_eq!(identity.public_key_bytes().unwrap().len(), 32);
            assert_eq!(identity.private_key_bytes().unwrap().len(), 32);
        });
    }

    #[test]
    fn ensure_identity_returns_existing_when_did_matches() {
        with_test_dir(|_| {
            let (first, generated) = ensure_identity("did:plc:same", &mut OsRng).unwrap();
            assert!(generated);

            let (second, generated) = ensure_identity("did:plc:same", &mut OsRng).unwrap();
            assert!(!generated);
            assert_eq!(first.public_key, second.public_key);
            assert_eq!(first.private_key, second.private_key);
        });
    }

    #[test]
    fn ensure_identity_regenerates_on_did_mismatch() {
        with_test_dir(|_| {
            let (first, _) = ensure_identity("did:plc:alice", &mut OsRng).unwrap();
            let (second, generated) = ensure_identity("did:plc:bob", &mut OsRng).unwrap();
            assert!(generated);
            assert_eq!(second.did, "did:plc:bob");
            assert_ne!(first.public_key, second.public_key);
        });
    }

    #[test]
    fn load_identity_rejects_garbage_json() {
        with_test_dir(|_| {
            config::ensure_data_dir().unwrap();
            fs::write(file_path(), "not json {{{").unwrap();
            assert!(load_identity().is_err());
        });
    }

    #[test]
    fn load_identity_rejects_valid_json_wrong_schema() {
        with_test_dir(|_| {
            config::ensure_data_dir().unwrap();
            fs::write(file_path(), r#"{"color": "blue"}"#).unwrap();
            assert!(load_identity().is_err());
        });
    }

    #[test]
    fn load_identity_rejects_empty_file() {
        with_test_dir(|_| {
            config::ensure_data_dir().unwrap();
            fs::write(file_path(), "").unwrap();
            assert!(load_identity().is_err());
        });
    }

    #[test]
    fn load_identity_rejects_binary_noise() {
        with_test_dir(|_| {
            config::ensure_data_dir().unwrap();
            fs::write(file_path(), vec![0xFF, 0xFE, 0x00, 0x01]).unwrap();
            assert!(load_identity().is_err());
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
            public_key: BASE64.encode([0u8; 16]), // 16 bytes, not 32
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
            private_key: BASE64.encode([0u8; 64]), // 64 bytes, not 32
        };
        let err = identity.private_key_bytes().unwrap_err().to_string();
        assert!(err.contains("64 bytes"), "expected length in error: {err}");
    }
}
