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
//! contexts should be full-synced from the appview, and the SSE stream
//! resumes from current state.
//!
//! [`install_cabinet_tree`]: TreeKeeper::install_cabinet_tree
//! [`install_workspace_tree`]: TreeKeeper::install_workspace_tree

use std::collections::HashMap;

use crate::crypto::{ContentKey, X25519PrivateKey};
use crate::directories::{DecryptionCtx, DirectoryTree, TreeChange};
use crate::error::Error;
use crate::sse::events::{SseDirectoryRecord, SseEvent};

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

/// A persistent tree for one context (cabinet or workspace).
struct HeldTree {
    tree: DirectoryTree,
    /// For cabinet trees: the private X25519 key for direct key unwrapping.
    /// For workspace trees: None.
    private_key: Option<X25519PrivateKey>,
    /// For workspace trees: single-entry map of keyring URI → group key.
    /// For cabinet trees: empty.
    group_keys: HashMap<String, ContentKey>,
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
    pub fn install_cabinet_tree(&mut self, tree: DirectoryTree, private_key: X25519PrivateKey) {
        self.cabinet = Some(HeldTree {
            tree,
            private_key: Some(private_key),
            group_keys: HashMap::new(),
        });
    }

    /// Install a workspace tree. Replaces any previously-installed tree
    /// for the same keyring URI. `group_key` is the unwrapped content
    /// key for the workspace's keyring.
    pub fn install_workspace_tree(
        &mut self,
        keyring_uri: String,
        tree: DirectoryTree,
        group_key: ContentKey,
    ) {
        let mut group_keys = HashMap::new();
        group_keys.insert(keyring_uri.clone(), group_key);
        self.workspaces.insert(
            keyring_uri,
            HeldTree {
                tree,
                private_key: None,
                group_keys,
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
    /// Called on `stopSseConsumer` so that account switches don't leak
    /// a previous user's crypto material or metadata into the next
    /// session's address space.
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
        self.cabinet.as_ref().map(|h| &h.tree)
    }

    pub fn workspace_tree(&self, keyring_uri: &str) -> Option<&DirectoryTree> {
        self.workspaces.get(keyring_uri).map(|h| &h.tree)
    }

    fn tree_for_scope(&self, scope: &TreeScope) -> Option<&DirectoryTree> {
        match scope {
            TreeScope::Cabinet => self.cabinet_tree(),
            TreeScope::Workspace(uri) => self.workspace_tree(uri),
        }
    }

    // -- Event application --

    /// Apply one SSE event, patching the affected tree and firing
    /// watchers. Events for contexts not yet installed are dropped.
    pub fn apply_event(&mut self, event: &SseEvent) -> Result<(), Error> {
        match event {
            SseEvent::DirectoryUpsert(record) => self.apply_directory_upsert(record)?,
            SseEvent::DirectoryDelete(payload) => {
                if let Some(uri) = payload.best_uri() {
                    self.apply_directory_delete(uri)?;
                }
            }
            // Document upsert: documents aren't in the DirectoryTree — they're
            // leaf URIs referenced from parents — so no tree mutation runs.
            // But a document's encrypted metadata may have changed (rename,
            // tag edit, description update) and watchers need to know so
            // consumers can refetch metadata. Route by scope and fire every
            // watcher in that scope with the unchanged tree; the reload
            // debounce on the consumer side picks up the metadata delta.
            SseEvent::DocumentUpsert(record) => {
                let scope = match record.keyring_uri.as_deref() {
                    Some(uri) => TreeScope::Workspace(uri.to_string()),
                    None => TreeScope::Cabinet,
                };
                self.notify_scope(&scope);
            }
            // Document delete: handled implicitly by the companion
            // `DirectoryUpsert` that removes the entry from the parent
            // directory. Firing watchers here too would double-notify
            // without adding a signal the parent event doesn't already
            // carry. Cabinet deletions that somehow arrive without a
            // parent update are a single dropped refresh — the next SSE
            // event or navigation will catch them.
            SseEvent::DocumentDelete(_) => {}
            // Keyring events: TODO — detect rotation bump and call
            // invalidate_decrypted_names on the affected workspace tree.
            SseEvent::KeyringUpsert(_) | SseEvent::KeyringDelete(_) => {}
            // Grant events: don't affect the tree. Consumers can react
            // separately for sharing UI updates.
            SseEvent::GrantUpsert(_) | SseEvent::GrantDelete(_) => {}
            // Proposal events: routed upstream by the WASM SSE consumer
            // via `dispatch_proposal_sync`, not applied to the tree
            // directly. By the time they'd reach here they've already
            // been handled (or dropped as unroutable). TreeKeeper is
            // pure tree state — proposals require a sync round-trip
            // that can't happen under the keeper lock anyway.
            SseEvent::DirectoryUpdateUpsert(_)
            | SseEvent::DirectoryUpdateDelete(_)
            | SseEvent::KeyringUpdateUpsert(_)
            | SseEvent::KeyringUpdateDelete(_)
            | SseEvent::DocumentUpdateUpsert(_)
            | SseEvent::DocumentUpdateDelete(_) => {}
            // Reconnect: callers handle full-sync out-of-band. We just
            // fire all watchers so the UI repaints from current state
            // (which is stale until a full sync lands).
            SseEvent::Reconnect => {
                self.notify_all_watchers();
            }
        }
        Ok(())
    }

    fn apply_directory_upsert(&mut self, record: &SseDirectoryRecord) -> Result<(), Error> {
        let scope = match record.keyring_uri.as_deref() {
            Some(uri) => TreeScope::Workspace(uri.to_string()),
            None => TreeScope::Cabinet,
        };

        // Split the borrow across held's disjoint fields so apply_directory_delta
        // can take &mut tree while DecryptionCtx borrows &private_key and &group_keys.
        let did = self.did.as_str();
        let held = match &scope {
            TreeScope::Cabinet => self.cabinet.as_mut(),
            TreeScope::Workspace(uri) => self.workspaces.get_mut(uri),
        };

        let Some(held) = held else {
            log::debug!(
                "[tree_keeper] dropping event for unloaded context: {}",
                record.directory_uri
            );
            return Ok(());
        };

        let HeldTree {
            tree,
            private_key,
            group_keys,
        } = held;
        let ctx = DecryptionCtx {
            did,
            private_key: private_key.as_ref(),
            group_keys,
        };
        let change = tree.apply_directory_delta(record, &ctx)?;

        self.notify_watchers_for_change(&scope, &change);
        Ok(())
    }

    fn apply_directory_delete(&mut self, uri: &str) -> Result<(), Error> {
        // Delete payload has no keyring_uri. Try each context; only the
        // one containing the URI will report a change (others NoOp).
        let delete_payload = SseDirectoryRecord {
            directory_uri: uri.to_string(),
            owner_did: String::new(),
            entries: Vec::new(),
            encrypted_metadata: None,
            key_wrapping: None,
            keyring_uri: None,
            deleted_at: Some(String::new()),
            indexed_at: None,
        };

        let did = self.did.as_str();

        if let Some(held) = self.cabinet.as_mut() {
            let HeldTree {
                tree,
                private_key,
                group_keys,
            } = held;
            let ctx = DecryptionCtx {
                did,
                private_key: private_key.as_ref(),
                group_keys,
            };
            let change = tree.apply_directory_delta(&delete_payload, &ctx)?;
            if change.is_effective() {
                self.notify_watchers_for_change(&TreeScope::Cabinet, &change);
                return Ok(());
            }
        }

        // Collect workspace keys upfront to avoid borrowing conflicts.
        let keyring_uris: Vec<String> = self.workspaces.keys().cloned().collect();
        for keyring_uri in keyring_uris {
            let change = {
                let Some(held) = self.workspaces.get_mut(&keyring_uri) else {
                    continue;
                };
                let HeldTree {
                    tree,
                    private_key,
                    group_keys,
                } = held;
                let ctx = DecryptionCtx {
                    did,
                    private_key: private_key.as_ref(),
                    group_keys,
                };
                tree.apply_directory_delta(&delete_payload, &ctx)?
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

    // -- Watcher notification --

    /// Fire all watchers in the given scope. If the change was a deletion
    /// AND the deleted URI matches a watcher's directory_uri, that watcher
    /// gets a None notification and is auto-closed.
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
            TreeScope::Cabinet => cabinet.as_ref().map(|h| &h.tree),
            TreeScope::Workspace(uri) => workspaces.get(uri).map(|h| &h.tree),
        };
        let Some(tree) = tree else {
            return;
        };

        let deleted_uri = match change {
            TreeChange::Removed { uri } => Some(uri.as_str()),
            _ => None,
        };

        let mut auto_close: Vec<WatcherHandle> = Vec::new();

        for (handle, watcher) in watchers.iter_mut() {
            if &watcher.scope != scope {
                continue;
            }
            if deleted_uri == Some(watcher.directory_uri.as_str()) {
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
            TreeScope::Cabinet => cabinet.as_ref().map(|h| &h.tree),
            TreeScope::Workspace(uri) => workspaces.get(uri).map(|h| &h.tree),
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
    /// trees. Used on Reconnect.
    fn notify_all_watchers(&mut self) {
        let Self {
            cabinet,
            workspaces,
            watchers,
            ..
        } = self;

        for watcher in watchers.values_mut() {
            let tree = match &watcher.scope {
                TreeScope::Cabinet => cabinet.as_ref().map(|h| &h.tree),
                TreeScope::Workspace(uri) => workspaces.get(uri).map(|h| &h.tree),
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
