// Workspace: a shared file space backed by an `app.opake.keyring` record.
//
// In the AT Protocol layer, workspaces are keyrings — the underlying record
// type is `app.opake.keyring`. The `Workspace` type provides domain semantics
// over the keyring's crypto primitives: it's the aggregate root for shared
// documents, directories, and membership.
//
// Keyring is crypto plumbing. Workspace is the domain concept.

use crate::crypto::ContentKey;
use crate::directories::{workspace_root_directory_uri, workspace_root_rkey};

/// A shared file space. Represents the "open" state — the caller has already
/// authenticated and unwrapped the group key.
///
/// Zeroized on drop — holds the unwrapped group key.
#[derive(Clone, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct Workspace {
    /// The keyring AT-URI (e.g. `at://did:plc:owner/app.opake.keyring/abc`).
    #[zeroize(skip)]
    pub uri: String,
    /// Decrypted workspace name.
    #[zeroize(skip)]
    pub name: String,
    /// Decrypted description.
    #[zeroize(skip)]
    pub description: Option<String>,
    /// DID of the workspace owner (the keyring record's authority).
    #[zeroize(skip)]
    pub owner_did: String,
    /// Symmetric group key, unwrapped for the current user.
    pub key: ContentKey,
    /// Key rotation counter.
    #[zeroize(skip)]
    pub rotation: u64,
}

impl Workspace {
    /// Construct from keyring data after unwrapping the group key.
    pub fn from_keyring(
        uri: String,
        name: String,
        description: Option<String>,
        owner_did: String,
        key: ContentKey,
        rotation: u64,
    ) -> Self {
        Self {
            uri,
            name,
            description,
            owner_did,
            key,
            rotation,
        }
    }

    /// The underlying keyring AT-URI.
    pub fn keyring_uri(&self) -> &str {
        &self.uri
    }

    /// Deterministic rkey for this workspace's root directory: `ws-{keyring_rkey}`.
    pub fn root_rkey(&self) -> String {
        workspace_root_rkey(&self.uri)
    }

    /// AT-URI for this workspace's root directory on the owner's PDS.
    pub fn root_directory_uri(&self) -> String {
        workspace_root_directory_uri(&self.owner_did, &self.uri)
    }
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
