// Typed representations of the at.opake.* lexicon records.
//
// These mirror the lexicon JSON schemas and handle atproto's serialization
// conventions ($type discriminators, $bytes for binary data, $link for CIDs).
//
// AT Protocol primitives (AtUri, AtBytes, CidLink, BlobRef) live in the
// `atproto` module. The ones used as record fields are re-exported here.

mod account_config;
pub mod classify;
mod defs;
mod directory;
mod document;
mod grant;
mod keyring;
mod pair_request;
mod pair_response;
mod pending_share;
mod public_key;
pub mod vocabulary;

use crate::error::Error;

// Re-export atproto types that appear in record struct fields so that
// downstream code using `records::AtBytes` etc. keeps working.
pub use crate::atproto::{AtBytes, BlobRef, CidLink};

// Re-export all record types at the `records::` level.
pub use account_config::{
    AccountConfigRecord, AccountConfigUpdates, ACCOUNT_CONFIG_COLLECTION, ACCOUNT_CONFIG_RKEY,
};
pub use classify::{peek_version, UnreadableReason, UnreadableRef};
pub use defs::{
    DirectKeyWrapping, EncryptedMetadata, EncryptionEnvelope, KeyWrapping, KeyringKeyWrapping,
    KeyringMember, KeyringRef, Role, WrappedKey,
};
pub use directory::{entry_target_uri, Directory, ListingEntry};
pub use document::{DirectEncryption, Document, Encryption, KeyringEncryption};
pub use grant::Grant;
pub use keyring::{KeyHistoryEntry, Keyring};
pub use pair_request::{PairRequest, PAIR_REQUEST_ALGO, PAIR_REQUEST_COLLECTION};
pub use pair_response::{PairResponse, PAIR_RESPONSE_COLLECTION};
pub use pending_share::{PendingShare, PENDING_SHARE_COLLECTION};
pub use public_key::{PublicKeyRecord, PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY};

/// The current at.opake.* schema version this client understands. Records
/// with version <= this are compatible; higher versions must be rejected.
/// Owned by opake-crypto because the HKDF info string folds it in for domain
/// separation.
pub use opake_crypto::SCHEMA_VERSION;

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
    PendingShare,
);

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

    fn dummy_wrapped_key() -> WrappedKey {
        WrappedKey {
            did: "did:plc:test".into(),
            ciphertext: AtBytes {
                encoded: "AAAA".into(),
            },
            algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
        }
    }

    fn dummy_metadata() -> EncryptedMetadata {
        EncryptedMetadata {
            ciphertext: AtBytes {
                encoded: "AAAA".into(),
            },
            nonce: AtBytes {
                encoded: "BBBB".into(),
            },
        }
    }

    /// One serialized instance per record type. The protocol contract: every
    /// record carries `opakeVersion` as a top-level integer field, stable
    /// across schema versions.
    fn all_record_jsons() -> Vec<(&'static str, serde_json::Value)> {
        let t = "2026-03-01T00:00:00Z";
        let document = Document::new(
            crate::atproto::BlobRef {
                blob_type: "blob".into(),
                reference: CidLink { cid: "bafy".into() },
                mime_type: "application/octet-stream".into(),
                size: 1,
            },
            Encryption::Direct(DirectEncryption {
                envelope: EncryptionEnvelope {
                    algo: "aes-256-gcm".into(),
                    nonce: AtBytes {
                        encoded: "CCCC".into(),
                    },
                    keys: vec![dummy_wrapped_key()],
                },
            }),
            dummy_metadata(),
            t.into(),
        );
        let keyring = Keyring::new(
            vec![KeyringMember::with_wrap(dummy_wrapped_key(), Role::Manager)],
            dummy_metadata(),
            t.into(),
        );
        let pair_response = PairResponse {
            opake_version: SCHEMA_VERSION,
            request: "at://did:plc:test/at.opake.pairRequest/req".into(),
            wrapped_key: dummy_wrapped_key(),
            ciphertext: AtBytes {
                encoded: "AAAA".into(),
            },
            nonce: AtBytes {
                encoded: "BBBB".into(),
            },
            algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
            created_at: t.into(),
        };
        vec![
            (
                "accountConfig",
                serde_json::to_value(AccountConfigRecord::new(t)).unwrap(),
            ),
            (
                "directory",
                serde_json::to_value(dummy_encrypted_directory(t)).unwrap(),
            ),
            ("document", serde_json::to_value(document).unwrap()),
            (
                "grant",
                serde_json::to_value(Grant::new(
                    "at://did:plc:test/at.opake.document/doc".into(),
                    "did:plc:bob".into(),
                    dummy_wrapped_key(),
                    dummy_metadata(),
                    t.into(),
                ))
                .unwrap(),
            ),
            ("keyring", serde_json::to_value(keyring).unwrap()),
            (
                "pairRequest",
                serde_json::to_value(PairRequest::new(&[1u8; 32], &[2u8; 1184], t)).unwrap(),
            ),
            ("pairResponse", serde_json::to_value(pair_response).unwrap()),
            (
                "pendingShare",
                serde_json::to_value(PendingShare::new(
                    "at://did:plc:test/at.opake.document/doc".into(),
                    "did:plc:bob".into(),
                    dummy_metadata(),
                    t.into(),
                ))
                .unwrap(),
            ),
            (
                "publicKey",
                serde_json::to_value(PublicKeyRecord::new(&[3u8; 32], &[4u8; 1184], t)).unwrap(),
            ),
        ]
    }

    #[test]
    fn all_record_types_carry_top_level_opake_version() {
        let records = all_record_jsons();
        assert_eq!(records.len(), 9, "census must cover every record type");
        for (name, json) in records {
            let version = json.get("opakeVersion");
            assert!(
                version.is_some_and(|v| v.is_u64()),
                "{name} must carry top-level integer opakeVersion, got: {json}"
            );
        }
    }

    #[test]
    #[allow(non_snake_case)] // bug__ regression-naming convention
    fn bug__missing_opake_version_masked_as_current() {
        // Records without opakeVersion used to deserialize as the CURRENT
        // version via a serde default, masking absence. Absence is corrupt.
        for (name, mut json) in all_record_jsons() {
            json.as_object_mut().unwrap().remove("opakeVersion");
            let failed = match name {
                "accountConfig" => serde_json::from_value::<AccountConfigRecord>(json).is_err(),
                "directory" => serde_json::from_value::<Directory>(json).is_err(),
                "document" => serde_json::from_value::<Document>(json).is_err(),
                "grant" => serde_json::from_value::<Grant>(json).is_err(),
                "keyring" => serde_json::from_value::<Keyring>(json).is_err(),
                "pairRequest" => serde_json::from_value::<PairRequest>(json).is_err(),
                "pairResponse" => serde_json::from_value::<PairResponse>(json).is_err(),
                "pendingShare" => serde_json::from_value::<PendingShare>(json).is_err(),
                "publicKey" => serde_json::from_value::<PublicKeyRecord>(json).is_err(),
                _ => unreachable!("unknown record type {name}"),
            };
            assert!(failed, "{name} without opakeVersion must fail to parse");
        }
    }

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

    /// 1184-byte ML-KEM-768 public key for record-construction tests. The
    /// bytes are arbitrary — these tests don't exercise hybrid wrap, only
    /// the wire-format / serde shape.
    fn dummy_ml_kem_pubkey() -> [u8; 1184] {
        [0x42u8; 1184]
    }

    #[test]
    fn public_key_record_new_sets_defaults() {
        let record =
            PublicKeyRecord::new(&[42u8; 32], &dummy_ml_kem_pubkey(), "2026-03-01T00:00:00Z");
        assert_eq!(record.opake_version, SCHEMA_VERSION);
        assert_eq!(record.x25519_algo, "x25519");
        assert_eq!(record.ml_kem_algo, "ml-kem-768");
        assert_eq!(record.created_at, "2026-03-01T00:00:00Z");
    }

    #[test]
    fn public_key_record_roundtrips_through_json() {
        let record =
            PublicKeyRecord::new(&[7u8; 32], &dummy_ml_kem_pubkey(), "2026-03-01T12:00:00Z");
        let json = serde_json::to_string(&record).unwrap();
        let parsed: PublicKeyRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.opake_version, record.opake_version);
        assert_eq!(
            parsed.x25519_public_key.encoded,
            record.x25519_public_key.encoded
        );
        assert_eq!(parsed.x25519_algo, "x25519");
        assert_eq!(
            parsed.ml_kem_public_key.encoded,
            record.ml_kem_public_key.encoded
        );
        assert_eq!(parsed.ml_kem_algo, "ml-kem-768");
        assert_eq!(parsed.created_at, "2026-03-01T12:00:00Z");
    }

    #[test]
    fn public_key_record_uses_atbytes_wire_format() {
        let record =
            PublicKeyRecord::new(&[1u8; 32], &dummy_ml_kem_pubkey(), "2026-03-01T00:00:00Z");
        let json = serde_json::to_value(&record).unwrap();
        // atproto $bytes convention: { "$bytes": "<base64>" }
        assert!(json["x25519PublicKey"]["$bytes"].is_string());
        assert!(json["mlKemPublicKey"]["$bytes"].is_string());
    }

    fn dummy_encrypted_directory(created_at: &str) -> Directory {
        let key_wrapping = KeyWrapping::Direct(DirectKeyWrapping {
            keys: vec![WrappedKey {
                did: "did:plc:test".into(),
                ciphertext: AtBytes {
                    encoded: "AAAA".into(),
                },
                algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
            }],
        });
        let encrypted_metadata = EncryptedMetadata {
            ciphertext: AtBytes {
                encoded: "BBBB".into(),
            },
            nonce: AtBytes {
                encoded: "CCCC".into(),
            },
        };
        Directory::new(key_wrapping, encrypted_metadata, created_at.into())
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
        assert!(matches!(parsed.key_wrapping, KeyWrapping::Direct(_)));
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
            ListingEntry::new("at://did:plc:test/at.opake.document/abc", "bafydoc"),
            ListingEntry::new("at://did:plc:test/at.opake.directory/def", "bafydir"),
        ];
        directory.modified_at = Some("2026-03-01T12:00:00Z".into());

        let json = serde_json::to_string(&directory).unwrap();
        let parsed: Directory = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.entries.len(), 2);
        assert!(parsed.entries[0].target.contains("document"));
        assert!(parsed.entries[1].target.contains("directory"));
        assert_eq!(parsed.entries[0].target_cid.cid, "bafydoc");
        assert_eq!(parsed.modified_at.unwrap(), "2026-03-01T12:00:00Z");
    }

    #[test]
    fn directory_supersedes_omitted_when_none() {
        let directory = dummy_encrypted_directory("2026-03-01T00:00:00Z");
        let json = serde_json::to_value(&directory).unwrap();
        assert!(
            json.get("supersedes").is_none(),
            "absent supersedes should be omitted from serialization"
        );
    }

    #[test]
    fn directory_supersedes_roundtrips() {
        let mut directory = dummy_encrypted_directory("2026-03-01T00:00:00Z");
        directory.supersedes = Some("at://did:plc:test/at.opake.directory/prior".into());

        let json = serde_json::to_string(&directory).unwrap();
        let parsed: Directory = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.supersedes.as_deref(),
            Some("at://did:plc:test/at.opake.directory/prior")
        );
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
                "wrappedKey": {
                    "did": "did:plc:test",
                    "ciphertext": { "$bytes": "AAAA" },
                    "algo": "x25519-mlkem768-hkdf-a256kw-v2",
                },
                "role": "manager",
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
            vec![KeyringMember::with_wrap(
                WrappedKey {
                    did: "did:plc:test".into(),
                    ciphertext: AtBytes {
                        encoded: "AAAA".into(),
                    },
                    algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
                },
                Role::Manager,
            )],
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

    #[test]
    fn keyring_supersedes_roundtrips() {
        let mut keyring = Keyring::new(
            vec![KeyringMember::with_wrap(
                WrappedKey {
                    did: "did:plc:test".into(),
                    ciphertext: AtBytes {
                        encoded: "AAAA".into(),
                    },
                    algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
                },
                Role::Manager,
            )],
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
        keyring.supersedes = Some("at://did:plc:test/at.opake.keyring/prior".into());

        let json = serde_json::to_string(&keyring).unwrap();
        let parsed: Keyring = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.supersedes.as_deref(),
            Some("at://did:plc:test/at.opake.keyring/prior")
        );
    }
}
