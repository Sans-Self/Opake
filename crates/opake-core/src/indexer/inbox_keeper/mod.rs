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
//! [`WorkspaceKeeper`]: crate::indexer::workspace_keeper::WorkspaceKeeper
//! [`Opake::resolve_grant_metadata`]: crate::opake::Opake::resolve_grant_metadata

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::records::{UnreadableReason, UnreadableRef};

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
    pub author_did: String,
    pub document_uri: String,
    pub created_at: String,
}

/// An incoming grant that could not be read — its record was corrupt or written
/// by a newer schema version. Carried distinctly from the entry list so the
/// "Shared with me" view can signal an unreadable share without inventing one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UnreadableGrant {
    /// Grant record URI, when the envelope yielded one.
    pub uri: String,
    /// Corrupt vs needs-newer-client.
    pub reason: UnreadableReason,
}

/// A snapshot of the full inbox at one moment in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InboxSnapshot {
    pub entries: Vec<InboxEntry>,
    /// Grants skipped from `entries` because their record was unreadable.
    pub unreadable: Vec<UnreadableGrant>,
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
    /// Grant URI → reason for grants skipped as unreadable.
    unreadable: HashMap<String, UnreadableReason>,
    watchers: HashMap<InboxWatcherHandle, InboxWatcherCallback>,
    next_watcher_id: u64,
    loaded: bool,
}

impl InboxKeeper {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            unreadable: HashMap::new(),
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

    /// Number of grants currently signalled as unreadable.
    pub fn unreadable_count(&self) -> usize {
        self.unreadable.len()
    }

    /// Build a fresh snapshot. Entries and unreadable signals are each sorted
    /// by URI for stable iteration order.
    pub fn snapshot(&self) -> InboxSnapshot {
        let mut entries: Vec<InboxEntry> = self.entries.values().cloned().collect();
        entries.sort_by(|a, b| a.uri.cmp(&b.uri));
        let mut unreadable: Vec<UnreadableGrant> = self
            .unreadable
            .iter()
            .map(|(uri, &reason)| UnreadableGrant {
                uri: uri.clone(),
                reason,
            })
            .collect();
        unreadable.sort_by(|a, b| a.uri.cmp(&b.uri));
        InboxSnapshot {
            entries,
            unreadable,
            loaded: self.loaded,
        }
    }

    // -- Mutation API --

    /// Replace the entire entry set. Called after a full-list fetch
    /// from the indexer.
    ///
    /// Blind wholesale replace — the keeper does no snapshot-vs-stream
    /// reconciliation. A caller consuming a concurrent SSE stream owns the
    /// sequencing: serialize this against `upsert`/`delete` (buffer events
    /// that land during the fetch, replay after) or a delta arriving
    /// mid-fetch is lost to the stale snapshot. See the `opake-wasm`
    /// consumer for that policy.
    pub fn bootstrap(&mut self, entries: Vec<InboxEntry>) {
        self.bootstrap_with_signals(entries, &[]);
    }

    /// Replace the entry set AND the unreadable-grant signal set from a
    /// full-list fetch. Blind-replace, same contract as [`bootstrap`].
    ///
    /// [`bootstrap`]: Self::bootstrap
    pub fn bootstrap_with_signals(&mut self, entries: Vec<InboxEntry>, unreadable: &[UnreadableRef]) {
        self.entries = entries.into_iter().map(|e| (e.uri.clone(), e)).collect();
        self.unreadable = unreadable
            .iter()
            .filter_map(|r| r.uri.clone().map(|uri| (uri, r.reason)))
            .collect();
        self.loaded = true;
        self.notify();
    }

    /// Signal that a grant exists but could not be read (corrupt or
    /// future-version), keyed by grant URI. Fired by the SSE path so a poison
    /// grant upsert converges with the bootstrap state.
    pub fn signal_unreadable(&mut self, uri: &str, reason: UnreadableReason) {
        if self.unreadable.get(uri) == Some(&reason) {
            return;
        }
        self.unreadable.insert(uri.to_string(), reason);
        self.notify();
    }

    /// Insert or replace one entry. No-op if the new entry deep-equals
    /// the existing one — handles idempotent SSE echoes gracefully.
    pub fn upsert(&mut self, entry: InboxEntry) {
        let uri = entry.uri.clone();
        // A readable grant clears any prior unreadable signal for the same URI.
        let cleared = self.unreadable.remove(&uri).is_some();
        if let Some(existing) = self.entries.get(&uri) {
            if existing == &entry && !cleared {
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
    /// Called on `wipeState` so account switches don't leak the previous
    /// user's inbox into the next session's UI.
    pub fn uninstall_all(&mut self) {
        self.entries.clear();
        self.unreadable.clear();
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

/// Construct an [`InboxEntry`] from a grant envelope (used by both the SSE
/// upsert path and HTTP bootstrap). The envelope carries the verbatim PDS
/// grant record; the URI lives at the envelope level.
///
/// `recipient_did`: the caller's DID. Returns `None` when the grant isn't
/// for the caller (defense-in-depth — the indexer also routes by topic).
pub fn try_build_entry_from_envelope(
    envelope: &crate::indexer::types::IndexerEnvelope<crate::records::Grant>,
    recipient_did: &str,
) -> Option<InboxEntry> {
    if envelope.record.recipient != recipient_did {
        return None;
    }

    // Author DID is the authority portion of the grant URI.
    let author_did = envelope
        .uri
        .strip_prefix("at://")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or_default()
        .to_string();

    Some(InboxEntry {
        uri: envelope.uri.clone(),
        author_did,
        document_uri: envelope.record.document.clone(),
        created_at: envelope.record.created_at.clone(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
