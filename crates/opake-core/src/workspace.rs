// Workspace: a shared file space backed by an `app.opake.keyring` record.
//
// In the AT Protocol layer, workspaces are keyrings — the underlying record
// type is `app.opake.keyring`. The `Workspace` type provides domain semantics
// over the keyring's crypto primitives: it's the aggregate root for shared
// documents, directories, and membership.
//
// Keyring is crypto plumbing. Workspace is the domain concept.

use crate::crypto::{self, ContentKey, PrivateKeyBundle};
use crate::records::Keyring;

/// One historical group key, retained so documents encrypted under a
/// previous rotation can still be decrypted after the keyring rotates.
#[derive(Clone, Debug, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct HistoricalKey {
    #[zeroize(skip)]
    pub rotation: u64,
    pub key: ContentKey,
}

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
    /// Symmetric group key, unwrapped for the current user at the current rotation.
    pub key: ContentKey,
    /// Key rotation counter.
    #[zeroize(skip)]
    pub rotation: u64,
    /// Group keys for previous rotations, unwrapped from `keyHistory` for
    /// the current user. Empty when no rotations have happened. Documents
    /// uploaded before a rotation reference their original rotation in
    /// `keyringRef.rotation` and need the historical key to decrypt.
    pub historical_keys: Vec<HistoricalKey>,
}

impl Workspace {
    /// Construct from keyring data after unwrapping the group key.
    ///
    /// Intentionally `pub(crate)` — constructing a `Workspace` outside
    /// `opake-core` would let callers pass a group key + URI + owner that
    /// don't match reality, and `FileManager` would happily encrypt with
    /// the wrong key. The supported entry points are `Opake::resolve_workspace`,
    /// `Opake::file_context`, and the daemon's workspace-resolution helpers.
    pub(crate) fn from_keyring(
        uri: String,
        name: String,
        description: Option<String>,
        owner_did: String,
        key: ContentKey,
        rotation: u64,
        historical_keys: Vec<HistoricalKey>,
    ) -> Self {
        Self {
            uri,
            name,
            description,
            owner_did,
            key,
            rotation,
            historical_keys,
        }
    }

    /// The underlying keyring AT-URI.
    pub fn keyring_uri(&self) -> &str {
        &self.uri
    }

    /// Resolve the group key for a given rotation. Returns `None` when the
    /// caller wasn't a member at that rotation (no entry in `keyHistory`)
    /// or the rotation number is unknown.
    pub fn key_for_rotation(&self, rotation: u64) -> Option<&ContentKey> {
        self.group_keys().for_rotation(rotation)
    }

    /// Borrowed view of all key material for rotation-aware decryption.
    pub fn group_keys(&self) -> GroupKeys<'_> {
        GroupKeys {
            current_rotation: self.rotation,
            current: &self.key,
            historical: &self.historical_keys,
        }
    }
}

/// Borrowed view of the symmetric key material needed to decrypt content
/// in a workspace. Carries the current group key + rotation alongside
/// any historical keys the caller had access to.
///
/// Used by leaf decryption functions that need to pick the right group
/// key based on a document's `keyringRef.rotation` — passing a single
/// `&ContentKey` is wrong when the document predates the current rotation.
#[derive(Clone, Copy, Debug)]
pub struct GroupKeys<'a> {
    pub current_rotation: u64,
    pub current: &'a ContentKey,
    pub historical: &'a [HistoricalKey],
}

impl<'a> GroupKeys<'a> {
    /// Resolve the group key for a given rotation. Returns `None` when
    /// the caller wasn't a member at that rotation (no entry in
    /// `keyHistory`) or the rotation number is unknown.
    pub fn for_rotation(&self, rotation: u64) -> Option<&'a ContentKey> {
        if rotation == self.current_rotation {
            return Some(self.current);
        }
        self.historical
            .iter()
            .find(|h| h.rotation == rotation)
            .map(|h| &h.key)
    }
}

/// Unwrap each entry in the keyring's `keyHistory` for the caller and
/// return the resulting list of `(rotation, key)` pairs.
///
/// Entries the caller wasn't a member of (no member entry, or unwrap
/// fails) are silently dropped — historical readers may have left the
/// workspace before this caller joined, and that's normal. Wrap failures
/// for entries the caller *should* have access to surface as an empty
/// returned key for that rotation, which makes downstream decryption
/// fail loudly when it actually tries to use the missing rotation.
pub(crate) fn derive_historical_keys(
    keyring: &Keyring,
    did: &str,
    keyring_uri: &str,
    private_keys: &PrivateKeyBundle<'_>,
) -> Vec<HistoricalKey> {
    keyring
        .key_history
        .iter()
        .filter_map(|hist| {
            let member = hist.members.iter().find(|m| m.did() == did)?;
            let key = crypto::unwrap_key(
                &member.wrapped_key,
                private_keys,
                &crypto::WrapContext::Keyring { uri: keyring_uri },
            )
            .ok()?;
            Some(HistoricalKey {
                rotation: hist.rotation,
                key,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
