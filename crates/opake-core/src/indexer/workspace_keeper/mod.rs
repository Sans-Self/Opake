//! Persistent in-memory workspace list state driven by SSE events.
//!
//! `WorkspaceKeeper` is to the workspace list what [`TreeKeeper`] is to
//! directory trees: a single in-memory source of truth that receives
//! patches from SSE events and notifies subscribers with a typed
//! snapshot.
//!
//! This replaces the older "re-fetch `list_workspaces` on every SSE
//! keyring event" pattern, which relied on a round-trip through the
//! indexer and paid 1–4s of cursor-lag latency per update. With the
//! keeper, SSE events patch the list directly — the indexer is a
//! cold-start bootstrap path only.
//!
//! ## Cold-start
//!
//! The keeper is constructed empty and `loaded == false`. The first
//! full-list fetch (see `listWorkspaces` in the WASM layer) calls
//! [`WorkspaceKeeper::bootstrap`], which replaces the entry set and
//! flips `loaded = true`. Thereafter, individual [`SseEvent::KeyringUpsert`]
//! and [`SseEvent::KeyringDelete`] events apply incrementally.
//!
//! ## Membership changes
//!
//! When an `SseEvent::KeyringUpsert` arrives for a keyring this client
//! is no longer a member of (DID absent from the member list),
//! [`try_build_entry`] returns `None` and the consumer deletes the
//! workspace from the keeper. Transient key-unwrap failures return
//! `Some(entry)` with `name = None` rather than a delete — the
//! workspace stays visible and self-corrects on the next event.
//! See [`apply_keyring_record`] for the canonical dispatch logic.
//!
//! [`TreeKeeper`]: crate::indexer::tree_keeper::TreeKeeper
//! [`SseEvent::KeyringUpsert`]: crate::indexer::sse::events::SseEvent::KeyringUpsert
//! [`SseEvent::KeyringDelete`]: crate::indexer::sse::events::SseEvent::KeyringDelete

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::crypto::{self, KeyringMetadata, PrivateKeyBundle};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Opaque handle returned by [`WorkspaceKeeper::install_watcher`]. Pass
/// to [`WorkspaceKeeper::unwatch`] to stop receiving notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspaceWatcherHandle(u64);

/// A workspace's projected view state — platform-neutral, serializable.
///
/// Carries everything the UI layer needs to render a workspace in the
/// sidebar. The settings page fetches members on demand via
/// `listWorkspaceMembers` — they are not included here.
///
/// Two entries are considered equal (no watcher re-fire) when every
/// field matches — rotation alone isn't enough, since metadata can
/// change without a rotation bump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceEntry {
    /// The workspace's stable identity (genesis keyring URI).
    pub workspace_id: String,
    /// The current chain-head keyring URI. Equal to `workspace_id` for
    /// an un-superseded workspace; differs after a manager supersede.
    pub head_uri: String,
    pub rotation: u64,
    pub member_count: usize,
    pub created_at: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    /// The current user's role in this workspace (if a member).
    pub my_role: Option<String>,
}

/// A snapshot of the full workspace list at one moment in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceSnapshot {
    pub entries: Vec<WorkspaceEntry>,
    /// `true` once the keeper has been bootstrapped at least once. The
    /// initial watcher snapshot fires with `loaded == false` and an
    /// empty `entries` list so the UI can show a loading state rather
    /// than "no workspaces."
    pub loaded: bool,
}

/// Callback fired whenever the workspace list changes.
///
/// The keeper fires this synchronously inside the method that caused
/// the change (bootstrap, upsert, delete). Callbacks must not re-enter
/// the keeper.
pub type WorkspaceWatcherCallback = Box<dyn FnMut(&WorkspaceSnapshot)>;

// ---------------------------------------------------------------------------
// WorkspaceKeeper
// ---------------------------------------------------------------------------

/// Owns the workspace list and routes SSE keyring events to it.
pub struct WorkspaceKeeper {
    entries: HashMap<String, WorkspaceEntry>,
    watchers: HashMap<WorkspaceWatcherHandle, WorkspaceWatcherCallback>,
    next_watcher_id: u64,
    loaded: bool,
}

impl WorkspaceKeeper {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            watchers: HashMap::new(),
            next_watcher_id: 0,
            loaded: false,
        }
    }

    /// `true` once [`bootstrap`] has been called at least once.
    ///
    /// [`bootstrap`]: Self::bootstrap
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Current number of workspaces tracked.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Number of currently-registered watchers.
    pub fn watcher_count(&self) -> usize {
        self.watchers.len()
    }

    /// Look up a single entry by workspace ID.
    pub fn get(&self, workspace_id: &str) -> Option<&WorkspaceEntry> {
        self.entries.get(workspace_id)
    }

    /// Build a fresh snapshot. Entries are sorted by workspace ID for
    /// stable iteration order.
    pub fn snapshot(&self) -> WorkspaceSnapshot {
        let mut entries: Vec<WorkspaceEntry> = self.entries.values().cloned().collect();
        entries.sort_by(|a, b| a.workspace_id.cmp(&b.workspace_id));
        WorkspaceSnapshot {
            entries,
            loaded: self.loaded,
        }
    }

    // -- Mutation API --

    /// Replace the entire entry set. Called after a full-list fetch
    /// from the indexer.
    ///
    /// This is a blind wholesale replace: the keeper is a dumb in-memory
    /// projection and does no snapshot-vs-stream reconciliation. A caller
    /// that consumes a concurrent SSE stream owns the sequencing — it must
    /// serialize this against `upsert`/`delete` (e.g. buffer events that
    /// land during the fetch and replay them after) or a delta arriving
    /// mid-fetch will be clobbered by the stale snapshot. See the
    /// `opake-wasm` consumer for that policy.
    pub fn bootstrap(&mut self, entries: Vec<WorkspaceEntry>) {
        self.entries = entries
            .into_iter()
            .map(|e| (e.workspace_id.clone(), e))
            .collect();
        self.loaded = true;
        self.notify();
    }

    /// Insert or replace one entry. If the new entry deep-equals the
    /// existing one, no watchers fire — handles idempotent SSE echoes
    /// after a local write gracefully.
    pub fn upsert(&mut self, entry: WorkspaceEntry) {
        let workspace_id = entry.workspace_id.clone();
        if let Some(existing) = self.entries.get(&workspace_id) {
            if existing == &entry {
                return;
            }
        }
        self.entries.insert(workspace_id, entry);
        self.notify();
    }

    /// Remove an entry by workspace ID. No-op (no watcher fire) if it
    /// wasn't tracked.
    pub fn delete(&mut self, workspace_id: &str) {
        if self.entries.remove(workspace_id).is_some() {
            self.notify();
        }
    }

    /// Apply a rebuilt [`WorkspaceEntry`] derived from a keyring
    /// record event. `None` means the caller isn't a member (e.g.
    /// they were just rotated out) — we `delete` in that case so the
    /// sidebar drops the workspace.
    pub fn apply_keyring_record(&mut self, workspace_id: &str, entry: Option<WorkspaceEntry>) {
        match entry {
            Some(e) => self.upsert(e),
            None => self.delete(workspace_id),
        }
    }

    // -- Watcher API --

    /// Install a watcher. The callback fires **once immediately** with
    /// the current snapshot (matching the `watchDirectory` contract),
    /// and again on every subsequent change.
    pub fn install_watcher(
        &mut self,
        mut callback: WorkspaceWatcherCallback,
    ) -> WorkspaceWatcherHandle {
        let handle = WorkspaceWatcherHandle(self.next_watcher_id);
        self.next_watcher_id += 1;

        // Eager first snapshot.
        let snap = self.snapshot();
        callback(&snap);

        self.watchers.insert(handle, callback);
        handle
    }

    /// Remove a previously-installed watcher.
    pub fn unwatch(&mut self, handle: WorkspaceWatcherHandle) {
        self.watchers.remove(&handle);
    }

    /// Drop every entry, clear every watcher, and reset `loaded`.
    ///
    /// Called on `wipeState` so account switches don't leak the previous
    /// user's workspace list into the next session's UI.
    pub fn uninstall_all(&mut self) {
        self.entries.clear();
        self.watchers.clear();
        self.loaded = false;
    }

    fn notify(&mut self) {
        let snap = self.snapshot();
        for callback in self.watchers.values_mut() {
            callback(&snap);
        }
    }
}

impl Default for WorkspaceKeeper {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Entry builder
// ---------------------------------------------------------------------------

/// Construct a [`WorkspaceEntry`] from raw keyring fields.
///
/// Returns `None` only when the caller is provably not a member — their
/// DID is absent from the member list. Callers map `None` to a keeper
/// delete so the sidebar drops the workspace.
///
/// Key-unwrap failures (corrupt data, key material mismatch) return
/// `Some(entry)` with `name`/`description`/`icon` as `None`. The workspace
/// stays visible in the sidebar, just unnamed — matches the pre-refactor
/// `list_workspaces` behavior. A later SSE event or full bootstrap will
/// reconcile once the underlying issue resolves.
///
/// The metadata-decrypt step is independently best-effort: if the member
/// list unwraps but the encrypted metadata blob can't be decoded, the
/// visible fields are `None` for the same reason.
pub fn try_build_entry(
    envelope: &crate::indexer::types::IndexerEnvelope<crate::records::Keyring>,
    my_did: &str,
    private_keys: &PrivateKeyBundle<'_>,
) -> Option<WorkspaceEntry> {
    let keyring = &envelope.record;
    let head_uri = envelope.uri.as_str();
    let workspace_id = keyring.workspace_id.as_deref().unwrap_or(head_uri);

    // Locate our member entry. If not found, we're not a member.
    let my_member = keyring.members.iter().find(|m| m.did() == my_did)?;
    let my_role = my_member.role;
    let member_count = keyring.members.len();

    // Member wraps are anchored to the workspace's stable (genesis) URI, not
    // the head — unwrapping with `head_uri` breaks the moment the workspace
    // supersedes (add/remove member). `wrap_anchor` resolves the right URI
    // from the record itself.
    let group_key = match crypto::unwrap_key(
        &my_member.wrapped_key,
        private_keys,
        &crypto::WrapContext::Keyring {
            uri: keyring.wrap_anchor(head_uri),
        },
    ) {
        Ok(k) => k,
        Err(_) => {
            return Some(WorkspaceEntry {
                workspace_id: workspace_id.to_string(),
                head_uri: head_uri.to_string(),
                rotation: keyring.rotation,
                member_count,
                created_at: Some(keyring.created_at.clone()),
                name: None,
                description: None,
                icon: None,
                my_role: Some(my_role.to_string()),
            });
        }
    };

    let (name, description, icon) =
        match crypto::decrypt_metadata::<KeyringMetadata>(&group_key, &keyring.encrypted_metadata) {
            Ok(meta) => (Some(meta.name), meta.description, meta.icon),
            Err(_) => (None, None, None),
        };

    Some(WorkspaceEntry {
        workspace_id: workspace_id.to_string(),
        head_uri: head_uri.to_string(),
        rotation: keyring.rotation,
        member_count,
        created_at: Some(keyring.created_at.clone()),
        name,
        description,
        icon,
        my_role: Some(my_role.to_string()),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
