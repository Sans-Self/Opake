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

pub use add_member::add_member;
pub use create::{create_keyring, CreateKeyringParams};
pub use list::{list_keyrings, KeyringEntry};
pub use remove_member::{remove_member, MemberKey};

use crate::client::{Transport, XrpcClient};
use crate::error::Error;

pub const KEYRING_COLLECTION: &str = "app.opake.keyring";

/// Resolve a keyring name to its AT-URI by listing all keyrings and matching.
///
/// Errors if zero or multiple keyrings share the name.
pub async fn resolve_keyring_uri(
    client: &mut XrpcClient<impl Transport>,
    name: &str,
) -> Result<KeyringEntry, Error> {
    let keyrings = list_keyrings(client).await?;
    let matches: Vec<_> = keyrings.into_iter().filter(|k| k.name == name).collect();

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
