// Workspace: a shared file space backed by an `at.opake.keyring` record.
//
// In the AT Protocol layer, workspaces are keyrings — the underlying record
// type is `at.opake.keyring`. The `Workspace` type provides domain semantics
// over the keyring's crypto primitives: it's the aggregate root for shared
// documents, directories, and membership.
//
// Keyring is crypto plumbing. Workspace is the domain concept.

use std::fmt;

use crate::crypto::{self, ContentKey, PrivateKeyBundle};
use crate::records::{Keyring, Role};

/// The stable identity of a workspace — its genesis keyring AT-URI.
///
/// Two URI kinds exist for a workspace's keyring chain: the genesis URI
/// (this type) and the head URI (a plain `String`, churns on every
/// supersede). Three shipped bugs came from a call site handing the head
/// URI to something keyed on genesis; this type makes that a compile
/// error instead of a production incident.
///
/// No public constructor and no `From<String>`/`Deserialize` impl —
/// minting a `WorkspaceId` requires going through workspace resolution
/// (`Workspace::id`, `IndexerEnvelope<Keyring>::workspace_id`) or a
/// `pub(crate)` derivation site inside opake-core. JS-supplied strings at
/// the WASM boundary can never become one directly.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkspaceId(String);

impl WorkspaceId {
    /// Mint a `WorkspaceId` from an already-resolved genesis URI.
    ///
    /// `pub(crate)` — every call site inside opake-core has derived the
    /// value via `lineage_anchor` or an equivalent chain-genesis resolution,
    /// never a raw caller-supplied string.
    pub(crate) fn from_resolved(uri: impl Into<String>) -> Self {
        Self(uri.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for WorkspaceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Extract the DIDs of every member with `Role::Manager` from a keyring
/// record. Used at workspace-resolution time to populate the
/// `Workspace::manager_dids` field, which the directory read path
/// consults for the additivity check.
pub(crate) fn manager_dids_from_keyring(keyring: &Keyring) -> Vec<String> {
    keyring
        .members
        .iter()
        .filter(|m| matches!(m.role, Role::Manager))
        .map(|m| m.did().to_string())
        .collect()
}

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
    /// The keyring AT-URI (e.g. `at://did:plc:owner/at.opake.keyring/abc`).
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
    /// Symmetric group key for the current rotation. A member retained after
    /// a verification-driven exclusion has no current wrap, but can still
    /// hold historical keys.
    pub key: Option<ContentKey>,
    /// Key rotation counter.
    #[zeroize(skip)]
    pub rotation: u64,
    /// Group keys for previous rotations, unwrapped from `keyHistory` for
    /// the current user. Empty when no rotations have happened. Documents
    /// uploaded before a rotation reference their original rotation in
    /// `keyringRef.rotation` and need the historical key to decrypt.
    pub historical_keys: Vec<HistoricalKey>,
    /// DIDs of every member with `Role::Manager` in the current keyring
    /// head. Carried here so the directory read path can check the
    /// additivity rule ("editor-authored supersedes must add to the
    /// prior canonical's entry set; managers exempt") without re-fetching
    /// the keyring. Populated from the keyring record's `members` list
    /// at workspace-resolution time.
    #[zeroize(skip)]
    pub manager_dids: Vec<String>,
}

impl Workspace {
    /// Construct from keyring data after unwrapping the group key.
    ///
    /// Intentionally `pub(crate)` — constructing a `Workspace` outside
    /// `opake-core` would let callers pass a group key + URI + owner that
    /// don't match reality, and `FileManager` would happily encrypt with
    /// the wrong key. The supported entry points are `Opake::resolve_workspace`,
    /// `Opake::file_context`, and the daemon's workspace-resolution helpers.
    #[allow(clippy::too_many_arguments)] // Constructor — every field is a
                                         // workspace-identity-defining piece. Bundling into a struct would just
                                         // move the arg list one call up.
    pub(crate) fn from_keyring(
        uri: String,
        name: String,
        description: Option<String>,
        owner_did: String,
        key: impl Into<Option<ContentKey>>,
        rotation: u64,
        historical_keys: Vec<HistoricalKey>,
        manager_dids: Vec<String>,
    ) -> Self {
        Self {
            uri,
            name,
            description,
            owner_did,
            key: key.into(),
            rotation,
            historical_keys,
            manager_dids,
        }
    }

    /// True iff `did` is a manager of this workspace per the current
    /// keyring head's member list. Used by the additivity check on
    /// directory reads — managers are exempt from the "must add to
    /// prior canonical" rule.
    pub fn is_manager(&self, did: &str) -> bool {
        self.manager_dids.iter().any(|m| m == did)
    }

    /// The underlying keyring AT-URI.
    pub fn keyring_uri(&self) -> &str {
        &self.uri
    }

    /// The workspace's stable identity, typed.
    ///
    /// `uri` is already the genesis URI post-resolution (see
    /// `Opake::resolve_workspace_by_uri` / `resolve_foreign_workspace`) —
    /// this just wraps it as a `WorkspaceId` so genesis-keyed call sites
    /// can require the type instead of trusting every caller to pass the
    /// right string.
    pub fn id(&self) -> WorkspaceId {
        WorkspaceId::from_resolved(self.uri.clone())
    }

    /// Resolve the group key for a given rotation. Returns `None` when the
    /// caller wasn't a member at that rotation (no entry in `keyHistory`)
    /// or the rotation number is unknown.
    pub fn key_for_rotation(&self, rotation: u64) -> Option<&ContentKey> {
        self.group_keys().for_rotation(rotation)
    }

    /// The current group key, required for any operation that authors new
    /// workspace state. Historical material is deliberately never substituted
    /// here: doing so would encrypt new content under a retired generation.
    pub fn current_key(&self) -> Result<&ContentKey, crate::error::Error> {
        self.key
            .as_ref()
            .ok_or_else(|| crate::error::Error::CurrentGroupKeyUnavailable {
                workspace_id: self.uri.clone(),
            })
    }

    /// Borrowed view of all key material for rotation-aware decryption.
    pub fn group_keys(&self) -> GroupKeys<'_> {
        GroupKeys {
            current_rotation: self.rotation,
            current: self.key.as_ref(),
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
    pub current: Option<&'a ContentKey>,
    pub historical: &'a [HistoricalKey],
}

impl<'a> GroupKeys<'a> {
    /// Resolve the group key for a given rotation. Returns `None` when
    /// the caller wasn't a member at that rotation (no entry in
    /// `keyHistory`) or the rotation number is unknown.
    pub fn for_rotation(&self, rotation: u64) -> Option<&'a ContentKey> {
        if rotation == self.current_rotation {
            return self.current;
        }
        self.historical
            .iter()
            .find(|h| h.rotation == rotation)
            .map(|h| &h.key)
    }
}

/// Verify a keyring's declared lineage anchor against its key material.
///
/// The genesis rkey is derived from the rotation-0 group key and the
/// genesis authority's DID, so any member can check — offline, with the
/// keys they were handed — that the anchor really belongs to this key
/// material. A forged rkey fails on the key; a forged owner attribution
/// fails on the DID; a keyring whose rotation-0 key the caller cannot
/// resolve is unverifiable as that workspace for that caller.
// spec: workspace-identity § Identity adoption verifies by derivation
pub(crate) fn verify_workspace_identity(
    keyring: &Keyring,
    anchor: &str,
    current_key: Option<&ContentKey>,
    historical: &[HistoricalKey],
) -> bool {
    // spec: lineage § Lineage never flips across a supersede
    if keyring.lineage.is_some() && keyring.supersedes.is_none() {
        return false;
    }
    let Ok(at_uri) = crate::atproto::parse_at_uri(anchor) else {
        return false;
    };
    let keys = GroupKeys {
        current_rotation: keyring.rotation,
        current: current_key,
        historical,
    };
    let Some(rotation_zero) = keys.for_rotation(0) else {
        return false;
    };
    let expected = crypto::derive_workspace_identity_tag(rotation_zero, &at_uri.authority);
    expected == at_uri.rkey
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
    // History entries carry the prior rotations' member wraps, all anchored
    // to the workspace's stable (genesis) URI just like the live members.
    // `keyring_uri` is whatever URI the caller fetched the record at; the
    // record resolves its own anchor.
    let anchor = keyring.lineage_anchor(keyring_uri);
    keyring
        .key_history
        .iter()
        .filter_map(|hist| {
            let member = hist.members.iter().find(|m| m.did() == did)?;
            let key = crypto::unwrap_key(
                member.wrapped_key.as_ref()?,
                private_keys,
                &crypto::WrapContext::Keyring { uri: anchor },
                keyring.opake_version,
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
