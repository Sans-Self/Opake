use log::debug;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, ContentKey, CryptoRng, RngCore, X25519PublicKey};
use crate::error::Error;
use crate::records::{self, KeyHistoryEntry, Keyring};

use super::KEYRING_COLLECTION;

/// A remaining member's DID and public key, needed for re-wrapping.
pub struct MemberKey<'a> {
    pub did: &'a str,
    pub public_key: &'a X25519PublicKey,
}

/// Remove a member from a keyring, rotate the group key, and re-wrap to
/// remaining members.
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
    remaining_keys: &[MemberKey<'_>],
    modified_at: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<(ContentKey, u64), Error> {
    let at_uri = atproto::parse_at_uri(keyring_uri)?;

    debug!("fetching keyring record {}", keyring_uri);
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;

    let mut keyring: Keyring = serde_json::from_value(entry.value)?;
    records::check_version(keyring.version)?;

    let original_count = keyring.members.len();
    keyring.members.retain(|m| m.did != remove_did);

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

    debug!(
        "rotating group key, re-wrapping to {} remaining members",
        remaining_keys.len()
    );
    let did_keys: Vec<(&str, &X25519PublicKey)> = remaining_keys
        .iter()
        .map(|mk| (mk.did, mk.public_key))
        .collect();
    let (new_group_key, new_wrapped) = crypto::create_group_key(&did_keys, rng)?;

    keyring.members = new_wrapped;
    keyring.rotation += 1;
    keyring.modified_at = Some(modified_at.to_string());

    let new_rotation = keyring.rotation;

    debug!("updating keyring record (rotation {})", new_rotation);
    client
        .put_record(KEYRING_COLLECTION, &at_uri.rkey, &keyring)
        .await?;

    Ok((new_group_key, new_rotation))
}

#[cfg(test)]
#[path = "remove_member_tests.rs"]
mod tests;
