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

use crate::crypto::{self, KeyringMetadata, X25519PrivateKey};

pub const KEYRING_COLLECTION: &str = "app.opake.keyring";

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
pub fn decrypt_indexer_keyring_name(
    keyring: &crate::client::IndexerKeyring,
    did: &str,
    private_key: &X25519PrivateKey,
) -> Option<String> {
    let members: Vec<crate::records::KeyringMember> = keyring
        .members
        .iter()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect();
    let member = members.iter().find(|m| m.did() == did)?;
    let group_key = crypto::unwrap_key(&member.wrapped_key, private_key).ok()?;
    let encrypted_metadata: crate::records::EncryptedMetadata =
        serde_json::from_value(keyring.encrypted_metadata.clone()?).ok()?;
    let metadata: KeyringMetadata =
        crypto::decrypt_metadata(&group_key, &encrypted_metadata).ok()?;
    Some(metadata.name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod indexer_keyring_tests {
    use super::*;
    use crate::client::IndexerKeyring;
    use crate::crypto::{
        generate_content_key, wrap_key, OsRng, X25519DalekPublicKey, X25519DalekStaticSecret,
        X25519PrivateKey, X25519PublicKey,
    };
    use crate::records::KeyringMember;

    fn test_keypair() -> (X25519PublicKey, X25519PrivateKey) {
        let secret = X25519DalekStaticSecret::random_from_rng(OsRng);
        let public = X25519DalekPublicKey::from(&secret);
        (public.to_bytes(), secret.to_bytes())
    }

    /// Build an `IndexerKeyring` with real crypto: a group key wrapped to
    /// `member_did`'s public key and a `KeyringMetadata { name }` encrypted
    /// under the group key. Returns the keyring plus the caller's private
    /// key so tests can attempt decryption.
    fn fixture(
        name: &str,
        owner_did: &str,
        member_did: &str,
        uri: &str,
    ) -> (IndexerKeyring, X25519PrivateKey) {
        let (member_pubkey, member_privkey) = test_keypair();
        let group_key = generate_content_key(&mut OsRng);
        let wrapped =
            wrap_key(&group_key, &member_pubkey, member_did, &mut OsRng).expect("wrap_key");
        let encrypted = crypto::encrypt_metadata(
            &group_key,
            &KeyringMetadata {
                name: name.into(),
                description: None,
                icon: None,
                enforce_revocation: None,
            },
            &mut OsRng,
        )
        .expect("encrypt_metadata");

        let member = KeyringMember {
            wrapped_key: wrapped,
            role: crate::records::Role::Editor,
        };

        let keyring = IndexerKeyring {
            uri: uri.into(),
            owner_did: owner_did.into(),
            rotation: 0,
            members: vec![serde_json::to_value(&member).unwrap()],
            encrypted_metadata: Some(serde_json::to_value(&encrypted).unwrap()),
            created_at: Some("2026-04-14T00:00:00Z".into()),
            indexed_at: Some("2026-04-14T00:00:00Z".into()),
        };

        (keyring, member_privkey)
    }

    #[test]
    fn decrypts_name_for_member() {
        let member_did = "did:plc:member";
        let (keyring, privkey) = fixture(
            "family-photos",
            "did:plc:owner",
            member_did,
            "at://did:plc:owner/app.opake.keyring/abc",
        );

        let name = decrypt_indexer_keyring_name(&keyring, member_did, &privkey);
        assert_eq!(name.as_deref(), Some("family-photos"));
    }

    #[test]
    fn returns_none_when_not_a_member() {
        let member_did = "did:plc:member";
        let (keyring, _member_privkey) = fixture(
            "family-photos",
            "did:plc:owner",
            member_did,
            "at://did:plc:owner/app.opake.keyring/abc",
        );

        // A DID not present in the members list — we use an unrelated keypair
        // so that even if the DID matched, unwrap_key would fail.
        let (_, stranger_privkey) = test_keypair();
        let name = decrypt_indexer_keyring_name(&keyring, "did:plc:stranger", &stranger_privkey);
        assert!(name.is_none());
    }

    #[test]
    fn returns_none_when_wrong_private_key() {
        let member_did = "did:plc:member";
        let (keyring, _correct_privkey) = fixture(
            "family-photos",
            "did:plc:owner",
            member_did,
            "at://did:plc:owner/app.opake.keyring/abc",
        );

        // DID matches a member entry, but we unwrap with the wrong private key
        // — simulates an identity mismatch or corrupted local storage.
        let (_, wrong_privkey) = test_keypair();
        let name = decrypt_indexer_keyring_name(&keyring, member_did, &wrong_privkey);
        assert!(name.is_none());
    }

    #[test]
    fn returns_none_when_encrypted_metadata_missing() {
        let member_did = "did:plc:member";
        let (mut keyring, privkey) = fixture(
            "family-photos",
            "did:plc:owner",
            member_did,
            "at://did:plc:owner/app.opake.keyring/abc",
        );
        keyring.encrypted_metadata = None;

        let name = decrypt_indexer_keyring_name(&keyring, member_did, &privkey);
        assert!(name.is_none());
    }

    #[test]
    fn returns_none_when_member_json_wrong_shape() {
        let member_did = "did:plc:member";
        let (mut keyring, privkey) = fixture(
            "family-photos",
            "did:plc:owner",
            member_did,
            "at://did:plc:owner/app.opake.keyring/abc",
        );
        // Replace the (well-formed) member entry with valid JSON that
        // doesn't match the `KeyringMember` shape — `filter_map` drops
        // it silently and no member matches the DID.
        keyring.members = vec![serde_json::json!({"garbage": true})];

        let name = decrypt_indexer_keyring_name(&keyring, member_did, &privkey);
        assert!(name.is_none());
    }
}
