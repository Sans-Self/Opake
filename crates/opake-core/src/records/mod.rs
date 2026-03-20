// Typed representations of the app.opake.* lexicon records.
//
// These mirror the lexicon JSON schemas and handle atproto's serialization
// conventions ($type discriminators, $bytes for binary data, $link for CIDs).
//
// AT Protocol primitives (AtUri, AtBytes, CidLink, BlobRef) live in the
// `atproto` module. The ones used as record fields are re-exported here.

mod account_config;
mod defs;
mod directory;
mod document;
mod grant;
mod keyring;
mod pair_request;
mod pair_response;
mod pending_share;
mod public_key;

use crate::error::Error;

// Re-export atproto types that appear in record struct fields so that
// downstream code using `records::AtBytes` etc. keeps working.
pub use crate::atproto::{AtBytes, BlobRef, CidLink};

// Re-export all record types at the `records::` level.
pub use account_config::{AccountConfigRecord, ACCOUNT_CONFIG_COLLECTION, ACCOUNT_CONFIG_RKEY};
pub use defs::{EncryptedMetadata, EncryptionEnvelope, KeyringRef, WrappedKey};
pub use directory::Directory;
pub use document::{DirectEncryption, Document, Encryption, KeyringEncryption};
pub use grant::Grant;
pub use keyring::{KeyHistoryEntry, Keyring};
pub use pair_request::{PairRequest, PAIR_REQUEST_COLLECTION};
pub use pair_response::{PairResponse, PAIR_RESPONSE_COLLECTION};
pub use pending_share::{PendingShare, PENDING_SHARE_COLLECTION};
pub use public_key::{PublicKeyRecord, PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY};

/// The current app.opake.* schema version this client understands.
/// Records with version <= this are compatible; higher versions must be rejected.
pub const SCHEMA_VERSION: u32 = 1;

/// Record types that carry a schema version number.
pub trait Versioned {
    fn opake_version(&self) -> u32;
}

macro_rules! impl_versioned {
    ($($ty:ty),+ $(,)?) => {
        $(impl Versioned for $ty {
            fn opake_version(&self) -> u32 { self.opake_version }
        })+
    };
}

impl_versioned!(
    AccountConfigRecord,
    Directory,
    Document,
    PublicKeyRecord,
    Grant,
    Keyring,
    PairRequest,
    PairResponse,
    PendingShare
);

fn default_version() -> u32 {
    SCHEMA_VERSION
}

/// Reject records written by a newer schema version than this client understands.
pub fn check_version(record_version: u32) -> Result<(), Error> {
    if record_version > SCHEMA_VERSION {
        return Err(Error::InvalidRecord(format!(
            "record schema version {record_version} is newer than supported version {SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_version_accepts_current() {
        assert!(check_version(SCHEMA_VERSION).is_ok());
    }

    #[test]
    fn check_version_accepts_v1() {
        assert!(check_version(1).is_ok());
    }

    #[test]
    fn check_version_rejects_one_above() {
        let err = check_version(SCHEMA_VERSION + 1).unwrap_err();
        assert!(matches!(err, Error::InvalidRecord(_)));
    }

    #[test]
    fn check_version_rejects_max() {
        assert!(check_version(u32::MAX).is_err());
    }

    #[test]
    fn public_key_record_new_sets_defaults() {
        let record = PublicKeyRecord::new(&[42u8; 32], "2026-03-01T00:00:00Z");
        assert_eq!(record.opake_version, SCHEMA_VERSION);
        assert_eq!(record.algo, "x25519");
        assert_eq!(record.created_at, "2026-03-01T00:00:00Z");
    }

    #[test]
    fn public_key_record_roundtrips_through_json() {
        let record = PublicKeyRecord::new(&[7u8; 32], "2026-03-01T12:00:00Z");
        let json = serde_json::to_string(&record).unwrap();
        let parsed: PublicKeyRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.opake_version, record.opake_version);
        assert_eq!(parsed.public_key.encoded, record.public_key.encoded);
        assert_eq!(parsed.algo, "x25519");
        assert_eq!(parsed.created_at, "2026-03-01T12:00:00Z");
    }

    #[test]
    fn public_key_record_uses_atbytes_wire_format() {
        let record = PublicKeyRecord::new(&[1u8; 32], "2026-03-01T00:00:00Z");
        let json = serde_json::to_value(&record).unwrap();
        // atproto $bytes convention: { "$bytes": "<base64>" }
        assert!(json["publicKey"]["$bytes"].is_string());
    }

    fn dummy_encrypted_directory(created_at: &str) -> Directory {
        let encryption = Encryption::Direct(DirectEncryption {
            envelope: EncryptionEnvelope {
                algo: "aes-256-gcm".into(),
                nonce: AtBytes::from_raw(&[0u8; 12]),
                keys: vec![WrappedKey {
                    did: "did:plc:test".into(),
                    ciphertext: AtBytes {
                        encoded: "AAAA".into(),
                    },
                    algo: "x25519-hkdf-a256kw".into(),
                }],
            },
        });
        let encrypted_metadata = EncryptedMetadata {
            ciphertext: AtBytes {
                encoded: "BBBB".into(),
            },
            nonce: AtBytes {
                encoded: "CCCC".into(),
            },
        };
        Directory::new(encryption, encrypted_metadata, created_at.into())
    }

    #[test]
    fn directory_roundtrips_through_json() {
        let directory = dummy_encrypted_directory("2026-03-01T00:00:00Z");
        let json = serde_json::to_string(&directory).unwrap();
        let parsed: Directory = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.opake_version, SCHEMA_VERSION);
        assert!(parsed.entries.is_empty());
        assert_eq!(parsed.created_at, "2026-03-01T00:00:00Z");
        assert!(parsed.modified_at.is_none());
        // Encryption envelope is present
        assert!(matches!(parsed.encryption, Encryption::Direct(_)));
    }

    #[test]
    fn directory_entries_omitted_when_empty() {
        let directory = dummy_encrypted_directory("2026-03-01T00:00:00Z");
        let json = serde_json::to_value(&directory).unwrap();
        assert!(
            json.get("entries").is_none(),
            "empty entries should be omitted from serialization"
        );
    }

    #[test]
    fn directory_with_entries_roundtrips() {
        let mut directory = dummy_encrypted_directory("2026-03-01T00:00:00Z");
        directory.entries = vec![
            "at://did:plc:test/app.opake.document/abc".into(),
            "at://did:plc:test/app.opake.directory/def".into(),
        ];
        directory.modified_at = Some("2026-03-01T12:00:00Z".into());

        let json = serde_json::to_string(&directory).unwrap();
        let parsed: Directory = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.entries.len(), 2);
        assert!(parsed.entries[0].contains("document"));
        assert!(parsed.entries[1].contains("directory"));
        assert_eq!(parsed.modified_at.unwrap(), "2026-03-01T12:00:00Z");
    }

    #[test]
    fn keyring_without_key_history_deserializes() {
        // Records created before key_history existed won't have the field.
        // Verify they deserialize to an empty vec.
        let json = serde_json::json!({
            "opakeVersion": 1,
            "algo": "aes-256-gcm",
            "members": [{
                "did": "did:plc:test",
                "ciphertext": { "$bytes": "AAAA" },
                "algo": "x25519-hkdf-a256kw",
            }],
            "rotation": 0,
            "encryptedMetadata": {
                "ciphertext": { "$bytes": "AAAA" },
                "nonce": { "$bytes": "AAAAAAAAAAAAAAAA" },
            },
            "createdAt": "2026-03-01T00:00:00Z",
        });

        let keyring: Keyring = serde_json::from_value(json).unwrap();
        assert!(keyring.key_history.is_empty());
    }

    #[test]
    fn keyring_key_history_omitted_when_empty() {
        let keyring = Keyring::new(
            vec![WrappedKey {
                did: "did:plc:test".into(),
                ciphertext: AtBytes {
                    encoded: "AAAA".into(),
                },
                algo: "x25519-hkdf-a256kw".into(),
            }],
            EncryptedMetadata {
                ciphertext: AtBytes {
                    encoded: "AAAA".into(),
                },
                nonce: AtBytes {
                    encoded: "BBBB".into(),
                },
            },
            "2026-03-01T00:00:00Z".into(),
        );

        let json = serde_json::to_value(&keyring).unwrap();
        assert!(
            json.get("keyHistory").is_none(),
            "empty key_history should be omitted from serialization"
        );
    }
}
