//! Persistent in-memory inbox state driven by SSE events.
//!
//! `InboxKeeper` is to the inbox list what [`WorkspaceKeeper`] is to the
//! workspace list: a single in-memory source of truth that receives
//! patches from SSE `grant:upsert` / `grant:delete` events and notifies
//! subscribers with a typed snapshot.
//!
//! ## Cold-start
//!
//! The keeper is constructed empty with `loaded == false`. The first
//! full-list fetch (see `listInbox` in the WASM layer) calls
//! [`InboxKeeper::bootstrap`], which replaces the entry set and flips
//! `loaded = true`. Thereafter, individual `SseEvent::GrantUpsert` /
//! `SseEvent::GrantDelete` events apply incrementally.
//!
//! ## Why no crypto
//!
//! Unlike the workspace keeper, inbox entries are already-resolved
//! indexer records — a `grant:upsert` event carries the URI, owner,
//! and document URI in plaintext. Metadata decryption still requires a
//! cross-PDS fetch via [`Opake::resolve_grant_metadata`], but that's
//! the consumer's job, not the keeper's.
//!
//! [`WorkspaceKeeper`]: crate::workspace_keeper::WorkspaceKeeper
//! [`Opake::resolve_grant_metadata`]: crate::opake::Opake::resolve_grant_metadata

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Opaque handle returned by [`InboxKeeper::install_watcher`]. Pass to
/// [`InboxKeeper::unwatch`] to stop receiving notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InboxWatcherHandle(u64);

/// An incoming grant as seen by the recipient — mirrors the indexer's
/// `InboxGrant` DTO but lives in this crate so the keeper stays
/// self-contained.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InboxEntry {
    pub uri: String,
    pub owner_did: String,
    pub document_uri: String,
    pub created_at: String,
}

/// A snapshot of the full inbox at one moment in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InboxSnapshot {
    pub entries: Vec<InboxEntry>,
    /// `true` once the keeper has been bootstrapped at least once. The
    /// initial watcher snapshot fires with `loaded == false` and an
    /// empty `entries` list so the UI can show a loading state.
    pub loaded: bool,
}

/// Callback fired whenever the inbox list changes. Must not re-enter
/// the keeper.
pub type InboxWatcherCallback = Box<dyn FnMut(&InboxSnapshot)>;

// ---------------------------------------------------------------------------
// InboxKeeper
// ---------------------------------------------------------------------------

/// Owns the inbox list and routes SSE grant events to it.
pub struct InboxKeeper {
    entries: HashMap<String, InboxEntry>,
    watchers: HashMap<InboxWatcherHandle, InboxWatcherCallback>,
    next_watcher_id: u64,
    loaded: bool,
}

impl InboxKeeper {
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

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn watcher_count(&self) -> usize {
        self.watchers.len()
    }

    pub fn get(&self, uri: &str) -> Option<&InboxEntry> {
        self.entries.get(uri)
    }

    /// Build a fresh snapshot. Entries are sorted by URI for stable
    /// iteration order.
    pub fn snapshot(&self) -> InboxSnapshot {
        let mut entries: Vec<InboxEntry> = self.entries.values().cloned().collect();
        entries.sort_by(|a, b| a.uri.cmp(&b.uri));
        InboxSnapshot {
            entries,
            loaded: self.loaded,
        }
    }

    // -- Mutation API --

    /// Replace the entire entry set. Called after a full-list fetch
    /// from the indexer.
    pub fn bootstrap(&mut self, entries: Vec<InboxEntry>) {
        self.entries = entries.into_iter().map(|e| (e.uri.clone(), e)).collect();
        self.loaded = true;
        self.notify();
    }

    /// Insert or replace one entry. No-op if the new entry deep-equals
    /// the existing one — handles idempotent SSE echoes gracefully.
    pub fn upsert(&mut self, entry: InboxEntry) {
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

    // -- Watcher API --

    /// Install a watcher. Fires once immediately with the current
    /// snapshot (matching the `watchDirectory` / `watchWorkspaces`
    /// contract), and again on every subsequent change.
    pub fn install_watcher(&mut self, mut callback: InboxWatcherCallback) -> InboxWatcherHandle {
        let handle = InboxWatcherHandle(self.next_watcher_id);
        self.next_watcher_id += 1;

        let snap = self.snapshot();
        callback(&snap);

        self.watchers.insert(handle, callback);
        handle
    }

    pub fn unwatch(&mut self, handle: InboxWatcherHandle) {
        self.watchers.remove(&handle);
    }

    /// Drop every entry, clear every watcher, and reset `loaded`.
    ///
    /// Called on `stopSseConsumer` so account switches don't leak the
    /// previous user's inbox into the next session's UI.
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

impl Default for InboxKeeper {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Entry builder
// ---------------------------------------------------------------------------

/// Construct an [`InboxEntry`] from an SSE grant record event.
///
/// `recipient_did`: the caller's DID — we use it to filter out events
/// where the caller is NOT the recipient (the broadcaster already
/// routes by DID topic, but defense-in-depth is cheap here).
pub fn try_build_entry_from_sse_record(
    record: &crate::sse::events::SseGrantRecord,
    recipient_did: &str,
) -> Option<InboxEntry> {
    // If the grant event carries an explicit recipient, verify it matches.
    // When absent (older payloads), trust the broadcaster's topic routing.
    if let Some(ref r) = record.recipient_did {
        if r != recipient_did {
            return None;
        }
    }

    Some(InboxEntry {
        uri: record.uri.clone(),
        owner_did: record.owner_did.clone(),
        document_uri: record.document_uri.clone(),
        created_at: record.created_at.clone().unwrap_or_default(),
    })
}

/// Convenience wrapper: build an entry from an indexer [`InboxGrant`].
///
/// [`InboxGrant`]: crate::client::InboxGrant
pub fn entry_from_indexer_grant(grant: &crate::client::InboxGrant) -> InboxEntry {
    InboxEntry {
        uri: grant.uri.clone(),
        owner_did: grant.owner_did.clone(),
        document_uri: grant.document_uri.clone(),
        created_at: grant.created_at.clone(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
