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
pub mod directory_update;
mod document;
mod document_update;
mod grant;
mod invitation;
mod invitation_acceptance;
mod keyring;
pub mod keyring_update;
mod pair_request;
mod pair_response;
mod pending_share;
mod public_key;

use crate::error::Error;

// Re-export atproto types that appear in record struct fields so that
// downstream code using `records::AtBytes` etc. keeps working.
pub use crate::atproto::{AtBytes, BlobRef, CidLink};

// Re-export all record types at the `records::` level.
pub use account_config::{
    AccountConfigRecord, AccountConfigUpdates, ACCOUNT_CONFIG_COLLECTION, ACCOUNT_CONFIG_RKEY,
};
pub use defs::{
    DirectKeyWrapping, EncryptedMetadata, EncryptionEnvelope, KeyWrapping, KeyringKeyWrapping,
    KeyringMember, KeyringRef, Role, WrappedKey,
};
pub use directory::Directory;
pub use directory_update::{DirectoryUpdate, DirectoryUpdateRecord, DIRECTORY_UPDATE_COLLECTION};
pub use document::{DirectEncryption, Document, Encryption, KeyringEncryption};
pub use document_update::{DocumentUpdate, DocumentUpdateRecord, DOCUMENT_UPDATE_COLLECTION};
pub use grant::Grant;
pub use invitation::{Invitation, INVITATION_COLLECTION};
pub use invitation_acceptance::{InvitationAcceptance, INVITATION_ACCEPTANCE_COLLECTION};
pub use keyring::{KeyHistoryEntry, Keyring};
pub use keyring_update::{KeyringUpdate, KeyringUpdateRecord, KEYRING_UPDATE_COLLECTION};
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
    DirectoryUpdateRecord,
    Document,
    DocumentUpdateRecord,
    PublicKeyRecord,
    Grant,
    Keyring,
    KeyringUpdateRecord,
    PairRequest,
    PairResponse,
    PendingShare,
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

    /// 1184-byte ML-KEM-768 public key for record-construction tests. The
    /// bytes are arbitrary — these tests don't exercise hybrid wrap, only
    /// the wire-format / serde shape.
    fn dummy_ml_kem_pubkey() -> [u8; 1184] {
        [0x42u8; 1184]
    }

    #[test]
    fn public_key_record_new_sets_defaults() {
        let record = PublicKeyRecord::new(
            &[42u8; 32],
            &dummy_ml_kem_pubkey(),
            "2026-03-01T00:00:00Z",
        );
        assert_eq!(record.opake_version, SCHEMA_VERSION);
        assert_eq!(record.x25519_algo, "x25519");
        assert_eq!(record.ml_kem_algo, "ml-kem-768");
        assert_eq!(record.created_at, "2026-03-01T00:00:00Z");
    }

    #[test]
    fn public_key_record_roundtrips_through_json() {
        let record = PublicKeyRecord::new(
            &[7u8; 32],
            &dummy_ml_kem_pubkey(),
            "2026-03-01T12:00:00Z",
        );
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
        let record = PublicKeyRecord::new(
            &[1u8; 32],
            &dummy_ml_kem_pubkey(),
            "2026-03-01T00:00:00Z",
        );
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
                algo: "x25519-hkdf-a256kw".into(),
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
            "owner": "did:plc:test",
            "members": [{
                "wrappedKey": {
                    "did": "did:plc:test",
                    "ciphertext": { "$bytes": "AAAA" },
                    "algo": "x25519-hkdf-a256kw",
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
            "did:plc:test".into(),
            vec![KeyringMember {
                wrapped_key: WrappedKey {
                    did: "did:plc:test".into(),
                    ciphertext: AtBytes {
                        encoded: "AAAA".into(),
                    },
                    algo: "x25519-hkdf-a256kw".into(),
                },
                role: Role::Manager,
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

    // -----------------------------------------------------------------------
    // DocumentUpdate serde
    // -----------------------------------------------------------------------

    fn dummy_blob_ref() -> BlobRef {
        BlobRef {
            blob_type: "blob".into(),
            reference: CidLink {
                cid: "bafytest".into(),
            },
            mime_type: "application/octet-stream".into(),
            size: 1024,
        }
    }

    fn dummy_encrypted_metadata() -> EncryptedMetadata {
        EncryptedMetadata {
            ciphertext: AtBytes {
                encoded: "AAAA".into(),
            },
            nonce: AtBytes {
                encoded: "BBBB".into(),
            },
        }
    }

    #[test]
    fn document_update_content_roundtrips() {
        let record = DocumentUpdateRecord::update_content(
            "at://did:plc:test/app.opake.document/abc".into(),
            dummy_blob_ref(),
            "2026-03-21T00:00:00Z".into(),
        );
        let json = serde_json::to_string(&record).unwrap();
        let parsed: DocumentUpdateRecord = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            parsed.update,
            DocumentUpdate::UpdateContent { .. }
        ));
    }

    #[test]
    fn document_update_metadata_roundtrips() {
        let record = DocumentUpdateRecord::update_metadata(
            "at://did:plc:test/app.opake.document/abc".into(),
            dummy_encrypted_metadata(),
            "2026-03-21T00:00:00Z".into(),
        );
        let json = serde_json::to_string(&record).unwrap();
        let parsed: DocumentUpdateRecord = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            parsed.update,
            DocumentUpdate::UpdateMetadata { .. }
        ));
    }

    #[test]
    fn document_update_supersede_roundtrips() {
        let record = DocumentUpdateRecord::supersede(
            "at://did:plc:test/app.opake.document/abc".into(),
            dummy_blob_ref(),
            dummy_encrypted_metadata(),
            "at://did:plc:test/app.opake.document/old".into(),
            "2026-03-21T00:00:00Z".into(),
        );
        let json = serde_json::to_string(&record).unwrap();
        let parsed: DocumentUpdateRecord = serde_json::from_str(&json).unwrap();
        match &parsed.update {
            DocumentUpdate::Supersede { supersedes, .. } => {
                assert_eq!(supersedes, "at://did:plc:test/app.opake.document/old");
            }
            _ => panic!("expected Supersede variant"),
        }
    }

    #[test]
    fn document_update_optional_fields_omitted() {
        let record = DocumentUpdateRecord::update_content(
            "at://did:plc:test/app.opake.document/abc".into(),
            dummy_blob_ref(),
            "2026-03-21T00:00:00Z".into(),
        );
        let json = serde_json::to_value(&record).unwrap();
        assert!(json.get("encryptedMetadata").is_none());
        assert!(json.get("supersedes").is_none());
    }

    // -----------------------------------------------------------------------
    // DirectoryUpdate serde
    // -----------------------------------------------------------------------

    #[test]
    fn directory_update_add_entry_roundtrips() {
        let record = DirectoryUpdateRecord::add_entry(
            "at://did:plc:test/app.opake.keyring/kr1".into(),
            "at://did:plc:test/app.opake.directory/dir1".into(),
            "at://did:plc:test/app.opake.document/doc1".into(),
            "2026-03-21T00:00:00Z".into(),
        );
        let json = serde_json::to_string(&record).unwrap();
        let parsed: DirectoryUpdateRecord = serde_json::from_str(&json).unwrap();
        match &parsed.update {
            DirectoryUpdate::AddEntry {
                directory, entry, ..
            } => {
                assert_eq!(directory, "at://did:plc:test/app.opake.directory/dir1");
                assert_eq!(entry, "at://did:plc:test/app.opake.document/doc1");
            }
            _ => panic!("expected AddEntry variant"),
        }
    }

    #[test]
    fn directory_update_move_entry_roundtrips() {
        let record = DirectoryUpdateRecord::move_entry(
            "at://did:plc:test/app.opake.keyring/kr1".into(),
            "at://did:plc:test/app.opake.directory/src".into(),
            "at://did:plc:test/app.opake.directory/dst".into(),
            "at://did:plc:test/app.opake.document/doc1".into(),
            "2026-03-21T00:00:00Z".into(),
        );
        let json = serde_json::to_string(&record).unwrap();
        let parsed: DirectoryUpdateRecord = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed.update, DirectoryUpdate::MoveEntry { .. }));
    }

    #[test]
    fn directory_update_create_directory_roundtrips() {
        let record = DirectoryUpdateRecord::create_directory(
            "at://did:plc:test/app.opake.keyring/kr1".into(),
            "at://did:plc:test/app.opake.directory/parent".into(),
            dummy_encrypted_metadata(),
            "2026-03-21T00:00:00Z".into(),
        );
        let json = serde_json::to_string(&record).unwrap();
        let parsed: DirectoryUpdateRecord = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            parsed.update,
            DirectoryUpdate::CreateDirectory { .. }
        ));
    }

    #[test]
    fn directory_update_delete_directory_roundtrips() {
        let record = DirectoryUpdateRecord::delete_directory(
            "at://did:plc:test/app.opake.keyring/kr1".into(),
            "at://did:plc:test/app.opake.directory/dir1".into(),
            "2026-03-21T00:00:00Z".into(),
        );
        let json = serde_json::to_string(&record).unwrap();
        let parsed: DirectoryUpdateRecord = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            parsed.update,
            DirectoryUpdate::DeleteDirectory { .. }
        ));
    }

    #[test]
    fn directory_update_rename_directory_roundtrips() {
        let record = DirectoryUpdateRecord::rename_directory(
            "at://did:plc:test/app.opake.keyring/kr1".into(),
            "at://did:plc:test/app.opake.directory/dir1".into(),
            dummy_encrypted_metadata(),
            "2026-03-21T00:00:00Z".into(),
        );
        let json = serde_json::to_string(&record).unwrap();
        let parsed: DirectoryUpdateRecord = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            parsed.update,
            DirectoryUpdate::RenameDirectory { .. }
        ));
    }

    #[test]
    fn directory_update_remove_entry_roundtrips() {
        let record = DirectoryUpdateRecord::remove_entry(
            "at://did:plc:test/app.opake.keyring/kr1".into(),
            "at://did:plc:test/app.opake.directory/dir1".into(),
            "at://did:plc:test/app.opake.document/doc1".into(),
            "2026-03-21T00:00:00Z".into(),
        );
        let json = serde_json::to_string(&record).unwrap();
        let parsed: DirectoryUpdateRecord = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed.update, DirectoryUpdate::RemoveEntry { .. }));
    }

    #[test]
    fn directory_update_optional_fields_omitted() {
        let record = DirectoryUpdateRecord::add_entry(
            "at://did:plc:test/app.opake.keyring/kr1".into(),
            "at://did:plc:test/app.opake.directory/dir1".into(),
            "at://did:plc:test/app.opake.document/doc1".into(),
            "2026-03-21T00:00:00Z".into(),
        );
        let json = serde_json::to_value(&record).unwrap();
        assert!(json.get("sourceDirectory").is_none());
        assert!(json.get("targetDirectory").is_none());
        assert!(json.get("parentDirectory").is_none());
        assert!(json.get("encryptedMetadata").is_none());
    }
}
