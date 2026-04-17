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

use crate::crypto::{self, KeyringMetadata, X25519PrivateKey};
use crate::records::{EncryptedMetadata, KeyringMember};

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
    pub uri: String,
    pub owner_did: String,
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

    /// Look up a single entry by keyring URI.
    pub fn get(&self, uri: &str) -> Option<&WorkspaceEntry> {
        self.entries.get(uri)
    }

    /// Build a fresh snapshot. Entries are sorted by URI for stable
    /// iteration order.
    pub fn snapshot(&self) -> WorkspaceSnapshot {
        let mut entries: Vec<WorkspaceEntry> = self.entries.values().cloned().collect();
        entries.sort_by(|a, b| a.uri.cmp(&b.uri));
        WorkspaceSnapshot {
            entries,
            loaded: self.loaded,
        }
    }

    // -- Mutation API --

    /// Replace the entire entry set. Called after a full-list fetch
    /// from the indexer.
    pub fn bootstrap(&mut self, entries: Vec<WorkspaceEntry>) {
        self.entries = entries.into_iter().map(|e| (e.uri.clone(), e)).collect();
        self.loaded = true;
        self.notify();
    }

    /// Insert or replace one entry. If the new entry deep-equals the
    /// existing one, no watchers fire — handles idempotent SSE echoes
    /// after a local write gracefully.
    pub fn upsert(&mut self, entry: WorkspaceEntry) {
        let uri = entry.uri.clone();
        if let Some(existing) = self.entries.get(&uri) {
            if existing == &entry {
                return;
            }
        }
        self.entries.insert(uri, entry);
        self.notify();
    }

    /// Remove an entry by URI. No-op (no watcher fire) if it wasn't
    /// tracked.
    pub fn delete(&mut self, uri: &str) {
        if self.entries.remove(uri).is_some() {
            self.notify();
        }
    }

    /// Apply a rebuilt [`WorkspaceEntry`] derived from a keyring
    /// record event. `None` means the caller isn't a member (e.g.
    /// they were just rotated out) — we `delete` in that case so the
    /// sidebar drops the workspace.
    pub fn apply_keyring_record(&mut self, uri: &str, entry: Option<WorkspaceEntry>) {
        match entry {
            Some(e) => self.upsert(e),
            None => self.delete(uri),
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
    /// Called on `stopSseConsumer` so account switches don't leak the
    /// previous user's workspace list into the next session's UI.
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
#[allow(clippy::too_many_arguments)]
pub fn try_build_entry(
    uri: &str,
    owner_did: &str,
    rotation: u64,
    raw_members: &[serde_json::Value],
    encrypted_metadata: Option<&serde_json::Value>,
    created_at: Option<&str>,
    my_did: &str,
    private_key: &X25519PrivateKey,
) -> Option<WorkspaceEntry> {
    // Parse member records. Anything that doesn't round-trip through the
    // `KeyringMember` shape is dropped — matches the existing
    // `listWorkspaces` behavior.
    let parsed_members: Vec<KeyringMember> = raw_members
        .iter()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect();

    // Locate our member entry. If not found, we're definitely not a
    // member — return None so the caller deletes from the keeper.
    let my_member = parsed_members.iter().find(|m| m.did() == my_did)?;
    let my_role = my_member.role;

    // Inlined rather than calling `Opake::unwrap_workspace_key`: that
    // method lives on the generic `impl<T, R, S> Opake<T, R, S>` block,
    // so calling it as a free function would require naming dummy
    // generics. The logic is two lines; duplicating avoids the dance.
    let group_key = match crypto::unwrap_key(&my_member.wrapped_key, private_key) {
        Ok(k) => k,
        Err(_) => {
            // Unwrap failed — corrupt data or wrong key material.
            // Keep the workspace visible without metadata; the next SSE
            // event or full bootstrap will reconcile.
            return Some(WorkspaceEntry {
                uri: uri.to_string(),
                owner_did: owner_did.to_string(),
                rotation,
                member_count: raw_members.len(),
                created_at: created_at.map(str::to_string),
                name: None,
                description: None,
                icon: None,
                my_role: Some(my_role.to_string()),
            });
        }
    };

    // Best-effort metadata decrypt.
    let (name, description, icon) = match encrypted_metadata {
        Some(em_json) => {
            let decoded = serde_json::from_value::<EncryptedMetadata>(em_json.clone())
                .ok()
                .and_then(|em| crypto::decrypt_metadata::<KeyringMetadata>(&group_key, &em).ok());
            match decoded {
                Some(meta) => (Some(meta.name), meta.description, meta.icon),
                None => (None, None, None),
            }
        }
        None => (None, None, None),
    };

    Some(WorkspaceEntry {
        uri: uri.to_string(),
        owner_did: owner_did.to_string(),
        rotation,
        member_count: raw_members.len(),
        created_at: created_at.map(str::to_string),
        name,
        description,
        icon,
        my_role: Some(my_role.to_string()),
    })
}

/// Convenience wrapper: build an entry from an [`IndexerKeyring`].
///
/// [`IndexerKeyring`]: crate::indexer::IndexerKeyring
pub fn try_build_entry_from_indexer_keyring(
    keyring: &crate::indexer::IndexerKeyring,
    my_did: &str,
    private_key: &X25519PrivateKey,
) -> Option<WorkspaceEntry> {
    try_build_entry(
        &keyring.uri,
        &keyring.owner_did,
        keyring.rotation,
        &keyring.members,
        keyring.encrypted_metadata.as_ref(),
        keyring.created_at.as_deref(),
        my_did,
        private_key,
    )
}

/// Convenience wrapper: build an entry from an [`SseKeyringRecord`].
///
/// `rotation` defaults to `0` when absent from the SSE payload (a
/// well-formed broadcaster always emits it, but the field is `Option`
/// in the wire type so we handle the gap defensively).
///
/// [`SseKeyringRecord`]: crate::indexer::sse::events::SseKeyringRecord
pub fn try_build_entry_from_sse_record(
    record: &crate::indexer::sse::events::SseKeyringRecord,
    my_did: &str,
    private_key: &X25519PrivateKey,
) -> Option<WorkspaceEntry> {
    try_build_entry(
        &record.uri,
        &record.owner_did,
        record.rotation.unwrap_or(0),
        &record.member_entries,
        record.encrypted_metadata.as_ref(),
        record.created_at.as_deref(),
        my_did,
        private_key,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
