//! Persistent in-memory tree state driven by SSE events.
//!
//! `TreeKeeper` holds a [`DirectoryTree`] per context (cabinet +
//! per-workspace) and applies SSE record events incrementally via
//! [`DirectoryTree::apply_directory_delta`]. Watchers register against
//! a specific directory URI and receive the updated tree whenever an
//! event affects it.
//!
//! This is the replacement for the "rebuild the tree on every load"
//! model — once a tree is installed, it stays alive and patches in
//! place as events arrive.
//!
//! ## Cold-start semantics
//!
//! Events for a context that hasn't been installed yet are silently
//! dropped. Consumers are expected to call [`install_cabinet_tree`] or
//! [`install_workspace_tree`] during the normal "load this view" flow
//! (typically after the existing `FileManager::load_tree` call). The
//! reconnect contract then covers any gap: on reconnect, all installed
//! contexts should be full-synced from the indexer, and the SSE stream
//! resumes from current state.
//!
//! [`install_cabinet_tree`]: TreeKeeper::install_cabinet_tree
//! [`install_workspace_tree`]: TreeKeeper::install_workspace_tree

use std::collections::HashMap;

use crate::crypto::{ContentKey, X25519PrivateKey};
use crate::directories::{DecryptionCtx, DirectoryTree, TreeChange};
use crate::error::Error;
use crate::indexer::sse::events::{CorruptScope, SseCorruptRecord, SseEvent};
use crate::indexer::types::IndexerEnvelope;
use crate::records::UnreadableRef;

/// Callback fired when a watched directory's tree state changes.
///
/// Receives `Some(&DirectoryTree)` for normal updates (the binding layer
/// builds whatever snapshot shape the platform needs) and `None` when
/// the watched directory itself has been deleted (the watcher auto-closes
/// after this call per the API contract — the delete notification is a
/// one-shot "gone" signal).
pub type WatcherCallback = Box<dyn FnMut(Option<&DirectoryTree>)>;

/// Opaque handle returned by [`TreeKeeper::watch_cabinet`] /
/// [`TreeKeeper::watch_workspace`]. Pass to [`TreeKeeper::unwatch`] to
/// remove the watcher explicitly; watchers also auto-close when their
/// watched directory is deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WatcherHandle(u64);

/// The caller's hybrid private key pair. Heap-allocated via `Box` inside
/// both `HeldTree` variants so the ~2432-byte ML-KEM half is stored once
/// behind a pointer rather than inline — Rust enum layout uses
/// `max(variant size)`, so inline keys would bloat every `HeldTree`.
///
/// Both contexts need the private keys: the cabinet unwraps direct-wrapped
/// directory content keys, and a workspace unwraps the freshly-minted group
/// key straight from a rotation event so the live projection adopts it
/// without an indexer refetch.
/// spec:key-rotation § Live projections adopt a rotation completely
///
/// Mirrors the zeroization pattern from `cabinet::Cabinet` so dropping
/// the `Box` cleanly wipes both halves.
#[derive(zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
struct HybridPrivateKeys {
    x25519: X25519PrivateKey,
    ml_kem: crate::crypto::MlKemPrivateKey,
}

/// A persistent tree for one context (cabinet or workspace).
///
/// The two variants are mutually exclusive by construction: `TreeKeeper`
/// only ever places `Cabinet` in `self.cabinet` and `Workspace` in
/// `self.workspaces`. Keeping them separate eliminates the prior
/// struct-with-Options shape where every workspace tree silently carried
/// `Option<[u8; 2400]> = None` for the ML-KEM private key.
enum HeldTree {
    Cabinet {
        tree: DirectoryTree,
        keys: Box<HybridPrivateKeys>,
    },
    Workspace {
        tree: DirectoryTree,
        /// Current rotation's group key.
        group_key: Option<ContentKey>,
        /// Last-seen rotation counter from the keyring record. A forward
        /// bump drives in-place adoption of the new group key.
        rotation: u64,
        /// Group keys for previous rotations the caller had access to.
        /// Lets SSE-driven directory deltas decrypt records that were
        /// encrypted before the latest rotation.
        historical_keys: Vec<crate::workspace::HistoricalKey>,
        /// The caller's private keys, retained so a rotation event can be
        /// adopted in place: the new group key is unwrapped straight from
        /// the event's keyring record rather than triggering a reload.
        keys: Box<HybridPrivateKeys>,
    },
}

impl HeldTree {
    fn tree(&self) -> &DirectoryTree {
        match self {
            Self::Cabinet { tree, .. } | Self::Workspace { tree, .. } => tree,
        }
    }
}

impl std::fmt::Debug for HeldTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cabinet { .. } => f.write_str("HeldTree::Cabinet { [key material redacted] }"),
            Self::Workspace { rotation, .. } => {
                write!(f, "HeldTree::Workspace {{ rotation: {rotation} }}")
            }
        }
    }
    // Zeroization on drop: `CabinetKeys` derives `ZeroizeOnDrop`, and
    // `Workspace`'s `ContentKey`s zero via their own `ZeroizeOnDrop`
    // through the `HashMap` drop. Both arms are covered without a manual
    // `impl Drop for HeldTree`.
}

/// Which context a watcher is attached to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum TreeScope {
    Cabinet,
    Workspace(String),
}

struct WatcherEntry {
    scope: TreeScope,
    directory_uri: String,
    callback: WatcherCallback,
}

/// Owns per-context directory trees and routes SSE events to them.
///
/// Watchers are registered against a specific directory URI. On each
/// applied event, all watchers in the affected context are invoked with
/// a reference to the updated tree. For the POC we don't do URI-targeted
/// filtering at this layer — React's reconciliation handles redundant
/// snapshots, and per-watcher precision can be added later via a
/// parent index.
pub struct TreeKeeper {
    did: String,
    cabinet: Option<HeldTree>,
    workspaces: HashMap<String, HeldTree>,
    watchers: HashMap<WatcherHandle, WatcherEntry>,
    next_watcher_id: u64,
}

impl TreeKeeper {
    pub fn new(did: impl Into<String>) -> Self {
        Self {
            did: did.into(),
            cabinet: None,
            workspaces: HashMap::new(),
            watchers: HashMap::new(),
            next_watcher_id: 0,
        }
    }

    pub fn did(&self) -> &str {
        &self.did
    }

    // -- Tree installation (called after the initial load) --

    /// Install a cabinet tree. Replaces any previously-installed cabinet.
    pub fn install_cabinet_tree(
        &mut self,
        tree: DirectoryTree,
        x25519_private_key: X25519PrivateKey,
        ml_kem_private_key: crate::crypto::MlKemPrivateKey,
    ) {
        self.cabinet = Some(HeldTree::Cabinet {
            tree,
            keys: Box::new(HybridPrivateKeys {
                x25519: x25519_private_key,
                ml_kem: ml_kem_private_key,
            }),
        });
    }

    /// Install a workspace tree. Replaces any previously-installed tree
    /// for the same keyring URI. `group_key` is the unwrapped content
    /// key at the current rotation; `historical_keys` covers any older
    /// rotations the caller had access to so SSE-driven directory
    /// deltas can decrypt records encrypted under previous keys.
    ///
    /// The caller's private keys are retained so a rotation event can be
    /// adopted in place (unwrapping the new group key from the event's
    /// keyring record) without a reload.
    #[allow(clippy::too_many_arguments)] // Each arg is a distinct piece of
                                         // workspace decryption state; bundling them into a struct
                                         // just moves the arg list one call up.
    pub fn install_workspace_tree(
        &mut self,
        keyring_uri: String,
        tree: DirectoryTree,
        group_key: impl Into<Option<ContentKey>>,
        rotation: u64,
        historical_keys: Vec<crate::workspace::HistoricalKey>,
        x25519_private_key: X25519PrivateKey,
        ml_kem_private_key: crate::crypto::MlKemPrivateKey,
    ) {
        self.workspaces.insert(
            keyring_uri,
            HeldTree::Workspace {
                tree,
                group_key: group_key.into(),
                rotation,
                historical_keys,
                keys: Box::new(HybridPrivateKeys {
                    x25519: x25519_private_key,
                    ml_kem: ml_kem_private_key,
                }),
            },
        );
    }

    /// Remove a tree for a specific context. Called on account switch
    /// or workspace removal. Also closes all watchers in that scope.
    pub fn uninstall_cabinet(&mut self) {
        self.cabinet = None;
        self.watchers.retain(|_, w| w.scope != TreeScope::Cabinet);
    }

    pub fn uninstall_workspace(&mut self, keyring_uri: &str) {
        self.workspaces.remove(keyring_uri);
        self.watchers
            .retain(|_, w| !matches!(&w.scope, TreeScope::Workspace(uri) if uri == keyring_uri));
    }

    /// Drain every installed tree and drop every watcher.
    ///
    /// Dropping the `HeldTree` entries triggers `ContentKey`'s
    /// `ZeroizeOnDrop` impl, wiping any cached group keys and cabinet
    /// private key from memory. The decrypted directory name cache on
    /// each `DirectoryTree` is freed along with the tree itself.
    ///
    /// Called on `wipeState` so that account switches don't leak a
    /// previous user's crypto material or metadata into the next
    /// session's address space. `stopSseConsumer` does not drain the
    /// keepers — callers that need to pause streaming without forcing
    /// a re-bootstrap should stop the consumer without wiping.
    pub fn uninstall_all(&mut self) {
        self.cabinet = None;
        self.workspaces.clear();
        self.watchers.clear();
    }

    // -- Watcher management --

    pub fn watch_cabinet(
        &mut self,
        directory_uri: String,
        callback: WatcherCallback,
    ) -> WatcherHandle {
        self.install_watcher(TreeScope::Cabinet, directory_uri, callback)
    }

    pub fn watch_workspace(
        &mut self,
        keyring_uri: String,
        directory_uri: String,
        callback: WatcherCallback,
    ) -> WatcherHandle {
        self.install_watcher(TreeScope::Workspace(keyring_uri), directory_uri, callback)
    }

    fn install_watcher(
        &mut self,
        scope: TreeScope,
        directory_uri: String,
        mut callback: WatcherCallback,
    ) -> WatcherHandle {
        let handle = WatcherHandle(self.next_watcher_id);
        self.next_watcher_id += 1;

        // Fire once immediately with the current state, if the tree is
        // already installed. Mirrors the "eager first snapshot" contract.
        if let Some(tree) = self.tree_for_scope(&scope) {
            callback(Some(tree));
        }

        self.watchers.insert(
            handle,
            WatcherEntry {
                scope,
                directory_uri,
                callback,
            },
        );
        handle
    }

    pub fn unwatch(&mut self, handle: WatcherHandle) {
        self.watchers.remove(&handle);
    }

    pub fn watcher_count(&self) -> usize {
        self.watchers.len()
    }

    // -- Read helpers for bindings / tests --

    pub fn cabinet_tree(&self) -> Option<&DirectoryTree> {
        self.cabinet.as_ref().map(|h| h.tree())
    }

    pub fn workspace_tree(&self, keyring_uri: &str) -> Option<&DirectoryTree> {
        self.workspaces.get(keyring_uri).map(|h| h.tree())
    }

    /// Keyring URIs of every currently-installed workspace tree. Lets a
    /// caller (e.g. the reconnect resync) discover which trees to re-load
    /// without reaching into the keeper's internals.
    pub fn installed_workspace_keyring_uris(&self) -> Vec<String> {
        self.workspaces.keys().cloned().collect()
    }

    fn tree_for_scope(&self, scope: &TreeScope) -> Option<&DirectoryTree> {
        match scope {
            TreeScope::Cabinet => self.cabinet_tree(),
            TreeScope::Workspace(uri) => self.workspace_tree(uri),
        }
    }

    /// Resolve an optional keyring URI to a TreeScope.
    ///
    /// Non-empty `Some(uri)` → workspace scope. `None` → cabinet scope.
    /// **Empty string** `Some("")` is treated as `None`: the
    /// broadcaster should never emit it, but `#[serde(default)]`
    /// deserialization rules mean an absent-or-empty field would
    /// otherwise land in `TreeScope::Workspace("")`, which matches
    /// no installed tree and drops the event silently. Routing to
    /// cabinet is the safer interpretation — it's visible (cabinet
    /// watchers fire) and any real workspace event with a malformed
    /// URI was already broken upstream.
    fn scope_from_keyring_uri(keyring_uri: Option<&str>) -> TreeScope {
        match keyring_uri {
            Some(uri) if !uri.is_empty() => TreeScope::Workspace(uri.to_string()),
            _ => TreeScope::Cabinet,
        }
    }

    // -- Event application --

    /// Apply one SSE event, patching the affected tree and firing
    /// watchers. Events for contexts not yet installed are dropped.
    pub fn apply_event(&mut self, event: &SseEvent) -> Result<(), Error> {
        match event {
            SseEvent::DirectoryUpsert(envelope) => self.apply_directory_upsert(envelope, false)?,
            SseEvent::DirectoryDelete(payload) => {
                self.apply_directory_delete(&payload.uri)?;
            }
            SseEvent::DocumentUpsert(envelope) => {
                let scope = Self::scope_from_keyring_uri(envelope.record.workspace_id.as_deref());
                self.notify_scope(&scope);
            }
            SseEvent::DocumentDelete(_) => {
                self.notify_all_watchers();
            }
            SseEvent::KeyringUpsert(envelope) => {
                let did = self.did.clone();
                let keyring_uri = envelope
                    .record
                    .lineage
                    .as_deref()
                    .unwrap_or(envelope.uri.as_str())
                    .to_string();
                if self.adopt_keyring_rotation(&keyring_uri, &did, envelope) {
                    let scope = TreeScope::Workspace(keyring_uri);
                    self.notify_scope(&scope);
                }
            }
            SseEvent::KeyringDelete(_) => {}
            SseEvent::GrantUpsert(_) | SseEvent::GrantDelete(_) => {}
            SseEvent::CorruptRecord(corrupt) => self.apply_corrupt_record(corrupt),
            SseEvent::ChainForked(fork) => {
                log::warn!(
                    "chain forked: workspace={} scope={} path={:?} your={} fork_point={} winner={} winner_cid={}",
                    fork.workspace_id,
                    fork.scope,
                    fork.path,
                    fork.your_uri,
                    fork.fork_point_uri,
                    fork.winner_uri,
                    fork.winner_cid,
                );
            }
            SseEvent::Reconnect => {
                self.notify_all_watchers();
            }
        }
        Ok(())
    }

    /// Adopt a keyring rotation into an installed workspace tree in place.
    ///
    /// On a forward rotation the new group key is unwrapped straight from
    /// the event's keyring record, the caller's historical keys are
    /// re-derived from the record's `keyHistory`, and every cached
    /// directory name is re-decrypted against the adopted key material —
    /// so the projection after the event equals what a fresh bootstrap
    /// would produce, with no reload. Returns `true` when the rotation was
    /// adopted (watchers should be notified), `false` for an uninstalled
    /// workspace, an unchanged authoritative head, or a caller who was
    /// rotated out (unwrap fails).
    ///
    /// spec:key-rotation § Live projections adopt a rotation completely
    fn adopt_keyring_rotation(
        &mut self,
        keyring_uri: &str,
        did: &str,
        envelope: &IndexerEnvelope<crate::records::Keyring>,
    ) -> bool {
        let new_rotation = envelope.record.rotation;
        let Some(HeldTree::Workspace {
            tree,
            group_key,
            rotation,
            historical_keys,
            keys,
        }) = self.workspaces.get_mut(keyring_uri)
        else {
            return false;
        };

        let record = &envelope.record;
        let anchor = record.lineage_anchor(envelope.uri.as_str());
        let bundle = crate::crypto::PrivateKeyBundle {
            x25519: &keys.x25519,
            ml_kem: &keys.ml_kem,
        };

        // Unwrap the freshly-minted group key for this caller from the
        // event record itself. A caller rotated out of the workspace has
        // no member entry — leave the projection untouched; the workspace
        // keeper drops the workspace from the sidebar.
        let Some(my_member) = record.members.iter().find(|m| m.did() == did) else {
            log::debug!(
                "[tree_keeper] rotation on {keyring_uri} no longer includes us; not adopting"
            );
            return false;
        };
        let new_group_key = match my_member
            .wrapped_key
            .as_ref()
            .map(|wrap| {
                crate::crypto::unwrap_key(
                    wrap,
                    &bundle,
                    &crate::crypto::WrapContext::Keyring { uri: anchor },
                    record.opake_version,
                )
            })
            .transpose()
        {
            Ok(key) => key,
            Err(e) => {
                log::warn!(
                    "[tree_keeper] failed to unwrap current group key for {keyring_uri}; adopting historical-only after identity proof: {e}"
                );
                None
            }
        };

        // The prior rotation's key rides along in the event's `keyHistory`
        // (the rotating manager snapshots it there), so re-deriving from
        // the record archives it without threading in-memory state.
        let new_historical =
            crate::workspace::derive_historical_keys(record, did, envelope.uri.as_str(), &bundle);
        if !crate::workspace::verify_workspace_identity(
            record,
            anchor,
            new_group_key.as_ref(),
            &new_historical,
        ) {
            log::warn!("[tree_keeper] refusing unverified workspace identity for {keyring_uri}");
            return false;
        }

        // Upserts are indexer-resolved chain heads.  A delete can roll the
        // chain back to an older rotation, or undo a same-rotation repair by
        // restoring a head without our wrap.  Treating the rotation number as
        // a monotonic event sequence would retain key material from that
        // deleted head.  Only an identical key state is a metadata-only
        // supersede and can be ignored.
        let same_current =
            group_key.as_ref().map(|key| key.0) == new_group_key.as_ref().map(|key| key.0);
        let same_history = historical_keys.len() == new_historical.len()
            && historical_keys
                .iter()
                .zip(&new_historical)
                .all(|(held, next)| held.rotation == next.rotation && held.key.0 == next.key.0);
        if *rotation == new_rotation && same_current && same_history {
            return false;
        }

        *group_key = new_group_key;
        *rotation = new_rotation;
        *historical_keys = new_historical;

        // Re-decrypt names in place against the adopted keys rather than
        // leaving them invalidated at "?".
        let view = crate::workspace::GroupKeys {
            current_rotation: *rotation,
            current: group_key.as_ref(),
            historical: historical_keys,
        };
        let keys_map = HashMap::from([(keyring_uri.to_string(), view)]);
        tree.decrypt_names_with_group_keys(did, &bundle, &keys_map);

        true
    }

    fn apply_directory_upsert(
        &mut self,
        envelope: &IndexerEnvelope<crate::records::Directory>,
        deleted: bool,
    ) -> Result<(), Error> {
        let scope = Self::scope_from_keyring_uri(envelope.record.workspace_id.as_deref());

        let did = self.did.as_str();
        let held = match &scope {
            TreeScope::Cabinet => self.cabinet.as_mut(),
            TreeScope::Workspace(uri) => self.workspaces.get_mut(uri),
        };

        let Some(held) = held else {
            log::debug!(
                "[tree_keeper] dropping event for unloaded context: {}",
                envelope.uri
            );
            return Ok(());
        };

        let change = if deleted {
            // Delete path doesn't need decryption context.
            match held {
                HeldTree::Cabinet { tree, .. } => tree.apply_directory_delete(&envelope.uri),
                HeldTree::Workspace { tree, .. } => tree.apply_directory_delete(&envelope.uri),
            }
        } else {
            match held {
                HeldTree::Cabinet { tree, keys } => {
                    let bundle = crate::crypto::PrivateKeyBundle {
                        x25519: &keys.x25519,
                        ml_kem: &keys.ml_kem,
                    };
                    tree.apply_directory_delta(
                        &envelope.uri,
                        &envelope.record,
                        &DecryptionCtx::cabinet(did, &bundle),
                    )?
                }
                HeldTree::Workspace {
                    tree,
                    group_key,
                    rotation,
                    historical_keys,
                    keys: _,
                } => {
                    let view = crate::workspace::GroupKeys {
                        current_rotation: *rotation,
                        current: group_key.as_ref(),
                        historical: historical_keys,
                    };
                    let mut keys_map = HashMap::new();
                    let keyring_uri = match &scope {
                        TreeScope::Workspace(uri) => uri.clone(),
                        _ => unreachable!("workspace HeldTree always implies Workspace scope"),
                    };
                    keys_map.insert(keyring_uri, view);
                    tree.apply_directory_delta(
                        &envelope.uri,
                        &envelope.record,
                        &DecryptionCtx::workspace(did, &keys_map),
                    )?
                }
            }
        };

        self.notify_watchers_for_change(&scope, &change);
        Ok(())
    }

    fn apply_directory_delete(&mut self, uri: &str) -> Result<(), Error> {
        // Delete payload has no workspace_id — we don't know which scope the
        // removed record belonged to. Try each in turn and notify when one
        // reports an effective change.
        if let Some(HeldTree::Cabinet { tree, .. }) = self.cabinet.as_mut() {
            let change = tree.apply_directory_delete(uri);
            if change.is_effective() {
                self.notify_watchers_for_change(&TreeScope::Cabinet, &change);
                return Ok(());
            }
        }

        let keyring_uris: Vec<String> = self.workspaces.keys().cloned().collect();
        for keyring_uri in keyring_uris {
            let change = {
                let Some(held) = self.workspaces.get_mut(&keyring_uri) else {
                    continue;
                };
                let HeldTree::Workspace { tree, .. } = held else {
                    continue;
                };
                tree.apply_directory_delete(uri)
            };
            if change.is_effective() {
                self.notify_watchers_for_change(
                    &TreeScope::Workspace(keyring_uri.clone()),
                    &change,
                );
                return Ok(());
            }
        }

        Ok(())
    }

    /// Apply a corrupt / future-version directory record delivered by SSE,
    /// producing the same placeholder state as the snapshot path would.
    ///
    /// A corrupt record body can't be trusted to name its workspace, so the URI
    /// is matched against the references already present in each installed tree
    /// — the authorized-snapshot invariant. The first tree that references the
    /// URI gets the placeholder (a directory is parented in exactly one place);
    /// a URI nothing references is count-only, upholding the no-out-of-scope
    /// -disclosure rule. Corrupt documents, keyrings and grants are not this
    /// keeper's domain and leave the trees untouched.
    ///
    /// spec:record-validity § SSE delivery matches snapshot delivery
    fn apply_corrupt_record(&mut self, corrupt: &SseCorruptRecord) {
        if corrupt.scope != CorruptScope::Directory {
            return;
        }
        let Some(uri) = corrupt.uri.as_deref() else {
            return; // count-only: no URI to hang a placeholder on
        };
        let reference = UnreadableRef {
            uri: Some(uri.to_string()),
            reason: corrupt.reason,
        };

        // Cabinet first.
        if let Some(HeldTree::Cabinet { tree, .. }) = self.cabinet.as_mut() {
            if tree.apply_unreadable_ref(&reference) {
                self.notify_scope(&TreeScope::Cabinet);
                return;
            }
        }

        // Then each installed workspace, in turn (mirrors `apply_directory_delete`).
        let keyring_uris: Vec<String> = self.workspaces.keys().cloned().collect();
        for keyring_uri in keyring_uris {
            let applied = {
                let Some(HeldTree::Workspace { tree, .. }) = self.workspaces.get_mut(&keyring_uri)
                else {
                    continue;
                };
                tree.apply_unreadable_ref(&reference)
            };
            if applied {
                self.notify_scope(&TreeScope::Workspace(keyring_uri));
                return;
            }
        }

        log::debug!(
            "[tree_keeper] corrupt directory {uri} not referenced by any installed tree; count-only"
        );
    }

    // -- Watcher notification --

    /// Fire all watchers in the given scope. On a deletion, any watcher
    /// whose directory can no longer be reached from the tree root gets a
    /// `None` notification and is auto-closed — that covers both the
    /// directly-deleted directory and every descendant orphaned by it
    /// (deleting a parent strands children whose parent record is now
    /// gone). All other watchers receive the updated tree.
    fn notify_watchers_for_change(&mut self, scope: &TreeScope, change: &TreeChange) {
        // Destructure self to split borrows: we need `&self.cabinet` /
        // `&self.workspaces` alongside `&mut self.watchers`.
        let Self {
            cabinet,
            workspaces,
            watchers,
            ..
        } = self;

        let tree = match scope {
            TreeScope::Cabinet => cabinet.as_ref().map(|h| h.tree()),
            TreeScope::Workspace(uri) => workspaces.get(uri).map(|h| h.tree()),
        };
        let Some(tree) = tree else {
            return;
        };

        // The deleted URI's parent may still list it, so an exact match is
        // checked explicitly; descendants are caught by reachability.
        let deleted_uri = match change {
            TreeChange::Removed { uri } => Some(uri.as_str()),
            _ => None,
        };

        let mut auto_close: Vec<WatcherHandle> = Vec::new();

        for (handle, watcher) in watchers.iter_mut() {
            if &watcher.scope != scope {
                continue;
            }
            // Exact-deleted always closes. Descendants are closed only
            // when the tree has a root to measure reachability against —
            // without one, every watcher would look unreachable, so we
            // fall back to exact-match-only rather than close them all.
            let gone = match deleted_uri {
                Some(deleted) => {
                    deleted == watcher.directory_uri.as_str()
                        || (tree.root_uri().is_some()
                            && !tree.is_reachable_from_root(&watcher.directory_uri))
                }
                None => false,
            };
            if gone {
                (watcher.callback)(None);
                auto_close.push(*handle);
            } else {
                (watcher.callback)(Some(tree));
            }
        }

        for handle in auto_close {
            watchers.remove(&handle);
        }
    }

    /// Fire every watcher in the given scope with the current tree
    /// snapshot, without any deletion handling. Used for non-structural
    /// events (document metadata changes) where the tree hasn't mutated
    /// but consumers still need to re-derive their view — typically by
    /// refetching document metadata through the FileManager.
    fn notify_scope(&mut self, scope: &TreeScope) {
        let Self {
            cabinet,
            workspaces,
            watchers,
            ..
        } = self;

        let tree = match scope {
            TreeScope::Cabinet => cabinet.as_ref().map(|h| h.tree()),
            TreeScope::Workspace(uri) => workspaces.get(uri).map(|h| h.tree()),
        };
        let Some(tree) = tree else {
            return;
        };

        for watcher in watchers.values_mut() {
            if &watcher.scope == scope {
                (watcher.callback)(Some(tree));
            }
        }
    }

    /// Fire all watchers with the current state of their respective
    /// trees. Used on Reconnect, and after an out-of-band resync of every
    /// installed tree (e.g. a chain fork) so consumers re-render against
    /// the refreshed content.
    pub fn notify_all_watchers(&mut self) {
        let Self {
            cabinet,
            workspaces,
            watchers,
            ..
        } = self;

        for watcher in watchers.values_mut() {
            let tree = match &watcher.scope {
                TreeScope::Cabinet => cabinet.as_ref().map(|h| h.tree()),
                TreeScope::Workspace(uri) => workspaces.get(uri).map(|h| h.tree()),
            };
            if let Some(tree) = tree {
                (watcher.callback)(Some(tree));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
