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

use crate::client::{Transport, XrpcClient};
use crate::crypto::{self, KeyringMetadata, X25519PrivateKey};
use crate::error::Error;

pub const KEYRING_COLLECTION: &str = "app.opake.keyring";

/// Resolve a keyring name to its AT-URI by listing all keyrings, decrypting
/// metadata, and matching by name.
///
/// Requires the caller's DID and private key to unwrap each keyring's group
/// key for metadata decryption. Errors if zero or multiple keyrings share
/// the name.
pub async fn resolve_keyring_uri(
    client: &mut XrpcClient<impl Transport>,
    name: &str,
    did: &str,
    private_key: &X25519PrivateKey,
) -> Result<KeyringEntry, Error> {
    let keyrings = list_keyrings(client).await?;
    let mut matches = Vec::new();

    for entry in keyrings {
        if let Some(decrypted_name) = decrypt_keyring_name(&entry, did, private_key) {
            if decrypted_name == name {
                matches.push(entry);
            }
        }
    }

    match matches.len() {
        0 => Err(Error::NotFound(format!("no keyring named {name:?}"))),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => {
            let uris: Vec<_> = matches.iter().map(|k| k.uri.clone()).collect();
            Err(Error::AmbiguousName {
                name: name.to_string(),
                count: n,
                uris,
            })
        }
    }
}

/// Decrypt a keyring entry's name from its encrypted metadata.
///
/// Returns `None` if the group key can't be unwrapped (not a member) or
/// metadata decryption fails.
pub fn decrypt_keyring_name(
    entry: &KeyringEntry,
    did: &str,
    private_key: &X25519PrivateKey,
) -> Option<String> {
    let member = entry.members.iter().find(|m| m.did() == did)?;
    let group_key = crypto::unwrap_key(&member.wrapped_key, private_key).ok()?;
    let metadata: KeyringMetadata =
        crypto::decrypt_metadata(&group_key, &entry.encrypted_metadata).ok()?;
    Some(metadata.name)
}

/// Decrypt a keyring name from a raw Keyring record using an already-unwrapped group key.
pub fn decrypt_keyring_name_from_record(
    keyring: &crate::records::Keyring,
    group_key: &crypto::ContentKey,
) -> Option<String> {
    let metadata: KeyringMetadata =
        crypto::decrypt_metadata(group_key, &keyring.encrypted_metadata).ok()?;
    Some(metadata.name)
}
