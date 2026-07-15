// Keyring operations: create, list, add/remove members.
//
// A keyring is a named group with a shared symmetric group key (GK), wrapped
// to each member's X25519 pubkey. Documents encrypted under a keyring have
// their content key wrapped under GK. Adding a member gives them access to
// all documents under that keyring without per-document changes.

mod add_member;
mod create;
mod list;
mod remove_member;

pub use add_member::{add_member, AddMemberParams};
pub use create::{create_keyring, CreateKeyringParams};
pub use list::{list_keyrings, KeyringEntry};
pub use remove_member::remove_member;

use crate::crypto::{self, KeyringMetadata, PrivateKeyBundle};

pub const KEYRING_COLLECTION: &str = "at.opake.keyring";

/// Decrypt a keyring name from a raw Keyring record using an already-unwrapped group key.
pub fn decrypt_keyring_name_from_record(
    keyring: &crate::records::Keyring,
    group_key: &crypto::ContentKey,
) -> Option<String> {
    let metadata: KeyringMetadata =
        crypto::decrypt_metadata(group_key, &keyring.encrypted_metadata).ok()?;
    Some(metadata.name)
}

/// Decrypt an indexer-sourced keyring's name.
///
/// Mirrors [`decrypt_keyring_name`] for the [`IndexerKeyring`] shape, so
/// `workspace ls` can show workspaces the user is a *member* of (where the
/// keyring record lives on another user's PDS and the caller's own
/// `listRecords` call doesn't see it).
///
/// Goes through [`crate::records::KeyringMember`] — same deserialization
/// path used by [`crate::Opake::sync_single_workspace`] — so wire-format
/// changes update one place.
///
/// # Trust model
///
/// The returned name is only as trustworthy as the indexer that served the
/// record. Workspace membership is publicly forgeable (anyone can wrap a
/// group key to the caller's published X25519 pubkey), so a compromised
/// indexer that also controls a keyring the caller has been silently
/// added to can spoof the name to steer name-based resolution at a later
/// `resolve_workspace("family-photos")` call. The write would then land as
/// a proposal encrypted under the attacker-controlled group key.
///
/// For callers with an own-PDS keyring matching the same name, the merge
/// logic in [`crate::Opake::resolve_workspace`] catches the collision and
/// raises `AmbiguousName` — the user must disambiguate by URI. For
/// foreign-only names there is currently **no cryptographic anchor** on
/// the name ↔ URI binding; the reader trusts the indexer for that map.
/// A future end-to-end signature layer (owner signs the keyring record
/// with their DID's signing key, client verifies via DID doc) would close
/// this gap without any API change here.
///
/// Returns `None` if the DID isn't a member, deserialization fails,
/// unwrapping fails, or metadata decryption fails.
pub fn decrypt_indexer_workspace_name(
    envelope: &crate::indexer::types::IndexerEnvelope<crate::records::Keyring>,
    did: &str,
    private_keys: &PrivateKeyBundle<'_>,
) -> Option<String> {
    let keyring = &envelope.record;
    let member = keyring.members.iter().find(|m| m.did() == did)?;
    let group_key = crypto::unwrap_key(
        &member.wrapped_key,
        private_keys,
        // Member wraps are anchored to the workspace's stable (genesis) URI,
        // which survives supersedes — not the head URI in `envelope.uri`.
        &crypto::WrapContext::Keyring {
            uri: keyring.wrap_anchor(&envelope.uri),
        },
    )
    .ok()?;
    let metadata: KeyringMetadata =
        crypto::decrypt_metadata(&group_key, &keyring.encrypted_metadata).ok()?;
    Some(metadata.name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod indexer_workspace_tests {
    use super::*;
    use crate::crypto::{generate_content_key, wrap_key, OsRng};
    use crate::indexer::types::IndexerEnvelope;
    use crate::records::{Keyring, KeyringMember};
    use crate::test_utils::TestKeys;

    /// Build a keyring envelope with real hybrid crypto: a group key
    /// wrapped to `member`'s public-key bundle and a
    /// [`KeyringMetadata { name }`] encrypted under the group key.
    /// `uri` is used as the envelope URI — the test fixtures model a
    /// genesis-only workspace (no supersedes yet).
    fn fixture(
        name: &str,
        member: &TestKeys,
        member_did: &str,
        uri: &str,
    ) -> IndexerEnvelope<Keyring> {
        let group_key = generate_content_key(&mut OsRng);
        let wrapped = wrap_key(
            &group_key,
            &member.public_keys(),
            member_did,
            &crate::crypto::WrapContext::Keyring { uri },
            &mut OsRng,
        )
        .expect("wrap_key");
        let encrypted = crypto::encrypt_metadata(
            &group_key,
            &KeyringMetadata {
                name: name.into(),
                description: None,
                icon: None,
            },
            &mut OsRng,
        )
        .expect("encrypt_metadata");

        let member_record = KeyringMember {
            wrapped_key: wrapped,
            role: crate::records::Role::Editor,
        };

        IndexerEnvelope {
            uri: uri.into(),
            record: Keyring {
                opake_version: crate::records::SCHEMA_VERSION,
                algo: "aes-256-gcm".into(),
                members: vec![member_record],
                rotation: 0,
                key_history: Vec::new(),
                encrypted_metadata: encrypted,
                supersedes: None,
                workspace_id: None,
                created_at: "2026-04-14T00:00:00Z".into(),
                modified_at: None,
            },
            indexed_at: "2026-04-14T00:00:00Z".into(),
            deleted_at: None,
        }
    }

    #[test]
    fn decrypts_name_for_member() {
        let member_did = "did:plc:member";
        let member = TestKeys::generate(member_did);
        let keyring = fixture(
            "family-photos",
            &member,
            member_did,
            "at://did:plc:owner/at.opake.keyring/abc",
        );

        let name = decrypt_indexer_workspace_name(&keyring, member_did, &member.private_keys());
        assert_eq!(name.as_deref(), Some("family-photos"));
    }

    #[test]
    fn returns_none_when_not_a_member() {
        let member_did = "did:plc:member";
        let member = TestKeys::generate(member_did);
        let keyring = fixture(
            "family-photos",
            &member,
            member_did,
            "at://did:plc:owner/at.opake.keyring/abc",
        );

        // A DID not present in the members list — we use unrelated keys
        // so that even if the DID matched, unwrap_key would fail.
        let stranger = TestKeys::generate("did:plc:stranger");
        let name =
            decrypt_indexer_workspace_name(&keyring, "did:plc:stranger", &stranger.private_keys());
        assert!(name.is_none());
    }

    #[test]
    fn returns_none_when_wrong_private_key() {
        let member_did = "did:plc:member";
        let member = TestKeys::generate(member_did);
        let keyring = fixture(
            "family-photos",
            &member,
            member_did,
            "at://did:plc:owner/at.opake.keyring/abc",
        );

        // DID matches a member entry, but we unwrap with the wrong private key
        // — simulates an identity mismatch or corrupted local storage.
        let wrong = TestKeys::generate(member_did);
        let name = decrypt_indexer_workspace_name(&keyring, member_did, &wrong.private_keys());
        assert!(name.is_none());
    }

    // `encrypted_metadata` and `members` JSON-shape tolerance tests have been
    // dropped along with the loose `IndexerWorkspace` DTO. The envelope now
    // wraps a strongly-typed `Keyring`; either deserialization succeeds and
    // both fields are well-formed, or the envelope never reaches this layer.
}
