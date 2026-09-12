use log::trace;

use std::collections::HashMap;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, CryptoRng, KeyringMetadata, RngCore};
use crate::error::Error;
use crate::records::{KeyHistoryEntry, Keyring};

use super::KEYRING_COLLECTION;

/// Remove a member from a keyring, rotate the group key, and re-wrap to
/// remaining members.
///
/// `old_group_key` is needed to decrypt the existing encrypted metadata so it
/// can be re-encrypted under the new group key.
///
/// Returns `(new_group_key, new_rotation)` — the caller must store the key
/// against the rotation number locally.
///
/// The old rotation's member entries (minus the removed member) are archived
/// into `key_history` so remaining members can still decrypt pre-rotation
/// documents.
///
/// `remaining_keys` must contain the public key for every member that will
/// remain *after* removal (including the owner). This is required because
/// the existing wrapped keys in the record are encrypted to *old* ephemeral
/// keys and can't be reused for the new group key.
pub async fn remove_member(
    client: &mut XrpcClient<impl Transport>,
    keyring_uri: &str,
    remove_did: &str,
    remaining_keys: &[crypto::DidMember<'_>],
    old_group_key: &ContentKey,
    modified_at: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<(ContentKey, u64), Error> {
    let at_uri = atproto::parse_at_uri(keyring_uri)?;

    let caller_did = client.did()?;
    if at_uri.authority != caller_did {
        return Err(Error::Auth(format!(
            "cannot modify keyring owned by {}, logged in as {caller_did}",
            at_uri.authority
        )));
    }

    trace!("fetching keyring record {}", keyring_uri);
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;

    let mut keyring: Keyring = serde_json::from_value(entry.value)?;
    // Write-strict: refuse to re-wrap a keyring newer than this client
    // understands, naming the keyring and the required remedy.
    super::guard_keyring_writable(keyring_uri, &keyring)?;

    let original_count = keyring.members.len();
    keyring.members.retain(|m| m.did() != remove_did);

    if keyring.members.len() == original_count {
        return Err(Error::InvalidRecord(format!(
            "{remove_did} is not a member of this keyring"
        )));
    }

    // Archive the current rotation's remaining member entries before replacing
    // them. The removed member is already gone (via retain above), so the
    // history only contains keys that remaining members can use.
    keyring.key_history.push(KeyHistoryEntry {
        rotation: keyring.rotation,
        members: keyring.members.clone(),
    });

    trace!(
        "rotating group key, re-wrapping to {} remaining members",
        remaining_keys.len()
    );
    let (new_group_key, new_wrapped) = crypto::create_group_key(remaining_keys, keyring_uri, rng)?;

    // Rebuild from the retained member records, not from produced wraps.
    // An omitted recipient has no new current wrap but remains admitted with
    // their role and approval intact; an old wrap must never be carried into
    // the new rotation as though it protected the new group key.
    let wraps_by_did: HashMap<String, _> = new_wrapped
        .into_iter()
        .map(|wrapped| (wrapped.did.clone(), wrapped))
        .collect();
    let new_members = keyring
        .members
        .iter()
        .cloned()
        .map(|mut member| {
            member.wrapped_key = wraps_by_did.get(member.did()).cloned();
            member
        })
        .collect();

    // Re-encrypt metadata: decrypt with old group key, encrypt with new one.
    // Both bind the keyring's lineage anchor (genesis URI), which the in-place
    // rewrite preserves.
    let anchor = crypto::SealContext::new(
        keyring.lineage_anchor(keyring_uri),
        crypto::SealType::KeyringMetadata,
    );
    let metadata: KeyringMetadata =
        crypto::decrypt_metadata(old_group_key, &keyring.encrypted_metadata, &anchor)?;
    keyring.encrypted_metadata = crypto::encrypt_metadata(&new_group_key, &metadata, &anchor, rng)?;

    keyring.members = new_members;
    keyring.rotation += 1;
    keyring.modified_at = Some(modified_at.to_string());

    let new_rotation = keyring.rotation;

    trace!("updating keyring record (rotation {})", new_rotation);
    client
        .put_record(KEYRING_COLLECTION, &at_uri.rkey, &keyring)
        .await?;

    Ok((new_group_key, new_rotation))
}

#[cfg(test)]
#[path = "remove_member_tests.rs"]
mod tests;
