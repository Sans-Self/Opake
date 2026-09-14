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
//! [`try_build_entry`] returns [`EntryOutcome::NotMember`] and the
//! consumer deletes the workspace from the keeper. A malformed or
//! undecryptable current member wrap is signalled as unreadable and does
//! not render a partial entry. A record whose declared identity fails the derivation check returns
//! [`EntryOutcome::IdentityMismatch`], which maps to no keeper operation
//! at all. See [`apply_keyring_record`] for the canonical dispatch logic.
//!
//! [`TreeKeeper`]: crate::indexer::tree_keeper::TreeKeeper
//! [`SseEvent::KeyringUpsert`]: crate::indexer::sse::events::SseEvent::KeyringUpsert
//! [`SseEvent::KeyringDelete`]: crate::indexer::sse::events::SseEvent::KeyringDelete

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::crypto::{self, KeyringMetadata, PrivateKeyBundle};
use crate::records::{UnreadableReason, UnreadableRef};

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

/// A workspace that exists but could not be read — its keyring record was
/// corrupt or written by a newer schema version. Carried DISTINCTLY from the
/// entry list so a client can tell "exists-but-unreadable" apart from "does not
/// exist" (see `record-validity` § corrupt workspaces are skipped with a
/// distinct signal). The name is never derived from the unreadable record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UnreadableWorkspace {
    /// Keyring URI of the skipped workspace (its stable identity when the
    /// keyring is genesis-shaped; the head URI otherwise — the only handle a
    /// corrupt record yields).
    pub uri: String,
    /// Corrupt vs needs-newer-client — lets the UI message each distinctly.
    pub reason: UnreadableReason,
}

/// A snapshot of the full workspace list at one moment in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceSnapshot {
    pub entries: Vec<WorkspaceEntry>,
    /// Workspaces skipped from `entries` because their keyring was unreadable.
    /// A distinct signal, never conflated with a workspace that does not exist.
    pub unreadable: Vec<UnreadableWorkspace>,
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
    /// Keyring URI → reason for workspaces skipped as unreadable. Distinct from
    /// `entries` (readable) and from absence (does not exist).
    unreadable: HashMap<String, UnreadableReason>,
    watchers: HashMap<WorkspaceWatcherHandle, WorkspaceWatcherCallback>,
    next_watcher_id: u64,
    loaded: bool,
}

impl WorkspaceKeeper {
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

    /// Number of workspaces currently signalled as existing-but-unreadable.
    pub fn unreadable_count(&self) -> usize {
        self.unreadable.len()
    }

    /// Build a fresh snapshot. Entries and unreadable signals are each sorted
    /// by URI for stable iteration order.
    pub fn snapshot(&self) -> WorkspaceSnapshot {
        let mut entries: Vec<WorkspaceEntry> = self.entries.values().cloned().collect();
        entries.sort_by(|a, b| a.workspace_id.cmp(&b.workspace_id));
        let mut unreadable: Vec<UnreadableWorkspace> = self
            .unreadable
            .iter()
            .map(|(uri, &reason)| UnreadableWorkspace {
                uri: uri.clone(),
                reason,
            })
            .collect();
        unreadable.sort_by(|a, b| a.uri.cmp(&b.uri));
        WorkspaceSnapshot {
            entries,
            unreadable,
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
        self.bootstrap_with_signals(entries, &[]);
    }

    /// Replace the entire entry set AND the unreadable-workspace signal set
    /// from a full-list fetch. A corrupt keyring in the listing appears here
    /// (skipped from `entries`, present in `unreadable`), so the client can
    /// show "this workspace exists but can't be read" distinctly from a
    /// workspace that isn't there.
    ///
    /// Same blind-replace contract as [`bootstrap`]: the caller owns
    /// snapshot-vs-stream sequencing.
    ///
    /// [`bootstrap`]: Self::bootstrap
    pub fn bootstrap_with_signals(
        &mut self,
        entries: Vec<WorkspaceEntry>,
        unreadable: &[UnreadableRef],
    ) {
        self.entries = entries
            .into_iter()
            .map(|e| (e.workspace_id.clone(), e))
            .collect();
        self.unreadable = unreadable
            .iter()
            .filter_map(|r| r.uri.clone().map(|uri| (uri, r.reason)))
            .collect();
        self.loaded = true;
        self.notify();
    }

    /// Signal that a workspace exists but its keyring record could not be read
    /// (corrupt or future-version), keyed by the keyring URI. Fired by the SSE
    /// path so a poison keyring upsert converges on the same state a bootstrap
    /// would produce. No-op (no watcher fire) if the signal is unchanged.
    ///
    /// spec:record-validity § SSE delivery matches snapshot delivery
    pub fn signal_unreadable(&mut self, uri: &str, reason: UnreadableReason) {
        if self.unreadable.get(uri) == Some(&reason) {
            return;
        }
        self.unreadable.insert(uri.to_string(), reason);
        self.notify();
    }

    /// Insert or replace one entry. If the new entry deep-equals the
    /// existing one, no watchers fire — handles idempotent SSE echoes
    /// after a local write gracefully.
    pub fn upsert(&mut self, entry: WorkspaceEntry) {
        let workspace_id = entry.workspace_id.clone();
        // A readable record clears any prior unreadable signal for this
        // workspace — keyed by both its stable id and its current head URI,
        // since a corrupt signal may have been recorded under either.
        let cleared = self.unreadable.remove(&workspace_id).is_some()
            | self.unreadable.remove(&entry.head_uri).is_some();
        if let Some(existing) = self.entries.get(&workspace_id) {
            if existing == &entry && !cleared {
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
    pub fn apply_keyring_record(&mut self, workspace_id: &str, outcome: EntryOutcome) {
        match outcome {
            EntryOutcome::Entry(e) => self.upsert(e),
            EntryOutcome::NotMember => self.delete(workspace_id),
            EntryOutcome::Unreadable { uri } => {
                let removed = self.entries.remove(workspace_id).is_some();
                let changed = self.unreadable.get(&uri) != Some(&UnreadableReason::Corrupt);
                if changed {
                    self.unreadable.insert(uri, UnreadableReason::Corrupt);
                }
                if removed || changed {
                    self.notify();
                }
            }
            // spec: workspace-identity § Identity adoption verifies by derivation
            EntryOutcome::IdentityMismatch => {}
        }
    }

    /// Apply a `keyring:delete` event per the indexer-resolved outcome —
    /// never by matching the deleted URI against tracked keys. The
    /// deleted URI equals a genesis-keyed entry whenever someone cleans
    /// up a genesis record, and dropping the entry then would kill a
    /// living workspace's sidebar.
    ///
    /// `Unchanged` is record cleanup. `RolledBack` is followed by a
    /// `keyring:upsert` of the restored head that rebuilds the entry
    /// through [`apply_keyring_record`]. Only `TornDown` — no live
    /// record remains in the chain — removes the entry, keyed by the
    /// payload's workspace identity.
    ///
    /// [`apply_keyring_record`]: Self::apply_keyring_record
    pub fn apply_keyring_delete(
        &mut self,
        payload: &crate::indexer::sse::events::SseKeyringDeletePayload,
    ) {
        use crate::indexer::sse::events::KeyringDeleteOutcome;

        match payload.outcome {
            KeyringDeleteOutcome::Unchanged | KeyringDeleteOutcome::RolledBack => {}
            KeyringDeleteOutcome::TornDown => self.delete(payload.workspace_id()),
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

impl Default for WorkspaceKeeper {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Entry builder
// ---------------------------------------------------------------------------

/// What a keyring record means for the keeper.
///
/// The three outcomes drive three different keeper operations, and the
/// distinction is load-bearing: `NotMember` maps to a delete (a removal
/// supersede must drop the sidebar entry), while `IdentityMismatch` maps
/// to *no operation at all* — a forged record must not be able to delete
/// or replace the real entry stored under the identity it declares.
// spec: workspace-identity § Identity adoption verifies by derivation
pub enum EntryOutcome {
    /// A verified, adoptable entry — insert or replace under its id.
    Entry(WorkspaceEntry),
    /// The caller's DID is absent from the member list — keeper delete.
    ///
    /// This outcome cannot carry a derivation check: verifying identity
    /// requires unwrapping the group key, which a non-member cannot do. The
    /// resulting delete is therefore guarded only by the indexer's write-time
    /// authority (a removal supersede must be manager-authored) plus the
    /// keeper's own no-op-on-untracked-id property — a forged `keyring:upsert`
    /// declaring a lineage the keeper does not already track deletes nothing.
    /// A direct-PDS keeper hydration path (none today) would need to
    /// re-establish that authority corroboration before honoring a delete.
    NotMember,
    /// The caller is a member, but their current encrypted group-key wrap is
    /// malformed or cannot be authenticated. It must be surfaced separately
    /// from absence so every adoption path fails closed in the same way.
    Unreadable { uri: String },
    /// The declared lineage anchor failed the identity derivation check.
    /// The record is dropped silently: no keeper operation, no rendered
    /// artifact, trace-level logging only.
    IdentityMismatch,
}

impl EntryOutcome {
    /// The adoptable entry, if any. Collapses `NotMember` and
    /// `IdentityMismatch` — correct only for bootstrap-style contexts
    /// where absence and drop coincide; event dispatch must match on the
    /// full outcome so the three keeper operations stay distinct.
    pub fn entry(self) -> Option<WorkspaceEntry> {
        match self {
            EntryOutcome::Entry(e) => Some(e),
            EntryOutcome::NotMember
            | EntryOutcome::Unreadable { .. }
            | EntryOutcome::IdentityMismatch => None,
        }
    }
}

/// Construct a [`WorkspaceEntry`] from raw keyring fields.
///
/// A corrupt or undecryptable current member wrap returns
/// [`EntryOutcome::Unreadable`]. It is never treated as an empty group key or
/// rendered as a nameless workspace; bootstrap and SSE both surface the same
/// distinct unreadable signal. A later valid head can replace that signal.
///
/// The metadata-decrypt step is independently best-effort: if the member
/// list unwraps but the encrypted metadata blob can't be decoded, the
/// visible fields are `None` for the same reason.
pub fn try_build_entry(
    envelope: &crate::indexer::types::IndexerEnvelope<crate::records::Keyring>,
    my_did: &str,
    private_keys: &PrivateKeyBundle<'_>,
) -> EntryOutcome {
    let keyring = &envelope.record;
    let head_uri = envelope.uri.as_str();
    let workspace_id = envelope.workspace_id();

    let Some(my_member) = keyring.members.iter().find(|m| m.did() == my_did) else {
        return EntryOutcome::NotMember;
    };
    let my_role = my_member.role.clone();
    let member_count = keyring.members.len();

    // Member wraps are anchored to the workspace's stable (genesis) URI, not
    // the head — unwrapping with `head_uri` breaks the moment the workspace
    // supersedes (add/remove member). `lineage_anchor` resolves the right URI
    // from the record itself.
    let historical =
        crate::workspace::derive_historical_keys(keyring, my_did, head_uri, private_keys);
    let group_key = match my_member.wrapped_key.as_ref() {
        None => None,
        Some(wrap) => match crypto::unwrap_key(
            wrap,
            private_keys,
            &crypto::WrapContext::Keyring {
                uri: keyring.lineage_anchor(head_uri),
            },
            keyring.opake_version,
        ) {
            Ok(key) => Some(key),
            Err(error) => {
                log::warn!("keyring at {head_uri} has an unreadable current member wrap: {error}");
                return EntryOutcome::Unreadable {
                    uri: head_uri.to_owned(),
                };
            }
        },
    };

    let anchor = keyring.lineage_anchor(head_uri);
    // spec: workspace-identity § Identity adoption verifies by derivation
    if !crate::workspace::verify_workspace_identity(
        keyring,
        anchor,
        group_key.as_ref(),
        &historical,
    ) {
        log::trace!("keyring at {head_uri} failed identity derivation for {anchor}; dropped");
        return EntryOutcome::IdentityMismatch;
    }

    let metadata_context = crypto::SealContext::new(
        keyring.lineage_anchor(head_uri),
        crypto::SealType::KeyringMetadata,
    );
    let (name, description, icon) = match group_key.as_ref().and_then(|key| {
        crypto::decrypt_metadata::<KeyringMetadata>(
            key,
            &keyring.encrypted_metadata,
            &metadata_context,
        )
        .ok()
    }) {
        Some(meta) => (Some(meta.name), meta.description, meta.icon),
        None => (None, None, None),
    };

    EntryOutcome::Entry(WorkspaceEntry {
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
