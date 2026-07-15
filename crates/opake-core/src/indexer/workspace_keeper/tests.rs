// WorkspaceKeeper unit tests.
//
// Keeper state/watcher lifecycle only — the crypto side (try_build_entry)
// is covered indirectly through the WASM listWorkspaces + SSE consumer
// integration path, which already has end-to-end tests.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::crypto::{CryptoRng, RngCore};

fn sample_entry(workspace_id: &str, rotation: u64) -> WorkspaceEntry {
    WorkspaceEntry {
        workspace_id: workspace_id.to_string(),
        head_uri: workspace_id.to_string(),
        rotation,
        member_count: 2,
        created_at: Some("2026-04-17T00:00:00Z".to_string()),
        name: Some("Test".to_string()),
        description: None,
        icon: None,
        my_role: Some("manager".to_string()),
    }
}

/// Capture every snapshot that fires through a watcher so tests can
/// assert on the delivered sequence.
fn capture_snapshots() -> (
    Rc<RefCell<Vec<WorkspaceSnapshot>>>,
    WorkspaceWatcherCallback,
) {
    let captured: Rc<RefCell<Vec<WorkspaceSnapshot>>> = Rc::new(RefCell::new(Vec::new()));
    let captured_clone = Rc::clone(&captured);
    let callback: WorkspaceWatcherCallback =
        Box::new(move |snap: &WorkspaceSnapshot| captured_clone.borrow_mut().push(snap.clone()));
    (captured, callback)
}

#[test]
fn new_keeper_is_empty_and_unloaded() {
    let keeper = WorkspaceKeeper::new();
    assert!(!keeper.is_loaded());
    assert_eq!(keeper.entry_count(), 0);
    assert_eq!(keeper.watcher_count(), 0);
}

#[test]
fn install_watcher_fires_initial_snapshot() {
    let mut keeper = WorkspaceKeeper::new();
    let (captured, callback) = capture_snapshots();
    let _handle = keeper.install_watcher(callback);

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 1, "watcher fires once immediately on install");
    assert!(!snaps[0].loaded);
    assert!(snaps[0].entries.is_empty());
}

#[test]
fn bootstrap_sets_loaded_and_notifies_watcher() {
    let mut keeper = WorkspaceKeeper::new();
    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    keeper.bootstrap(vec![
        sample_entry("at://a/kr/1", 1),
        sample_entry("at://a/kr/2", 1),
    ]);

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 2, "initial + post-bootstrap");
    let last = &snaps[1];
    assert!(last.loaded);
    assert_eq!(last.entries.len(), 2);
}

#[test]
// The keeper is patched only by indexer-derived inputs (bootstrap snapshot +
// SSE echo), and re-applying an already-present entry is a no-op — so a
// snapshot/stream overlap delivering the same record twice cannot double it.
// spec:indexer-consistency § Snapshot and stream jointly lose nothing
// spec:indexer-consistency § Client projections contain only indexer-confirmed state
fn upsert_with_identical_entry_does_not_refire() {
    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap(vec![sample_entry("at://a/kr/1", 1)]);

    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    // Re-apply the same entry — idempotent echo scenario.
    keeper.upsert(sample_entry("at://a/kr/1", 1));

    let snaps = captured.borrow();
    assert_eq!(
        snaps.len(),
        1,
        "initial snapshot only — no re-fire on no-op upsert"
    );
}

#[test]
fn upsert_with_changed_entry_refires() {
    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap(vec![sample_entry("at://a/kr/1", 1)]);

    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    // Rotation bump — fire.
    keeper.upsert(sample_entry("at://a/kr/1", 2));

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 2);
    assert_eq!(snaps[1].entries[0].rotation, 2);
}

#[test]
fn delete_removes_entry_and_fires() {
    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap(vec![
        sample_entry("at://a/kr/1", 1),
        sample_entry("at://a/kr/2", 1),
    ]);

    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    keeper.delete("at://a/kr/1");

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 2);
    assert_eq!(snaps[1].entries.len(), 1);
    assert_eq!(snaps[1].entries[0].workspace_id, "at://a/kr/2");
}

#[test]
fn delete_missing_uri_is_noop() {
    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap(vec![sample_entry("at://a/kr/1", 1)]);

    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    keeper.delete("at://a/kr/nonexistent");

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 1, "no fire when URI wasn't tracked");
}

#[test]
fn apply_keyring_record_none_deletes() {
    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap(vec![sample_entry("at://a/kr/1", 1)]);

    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    // "Record arrived but we're not a member anymore" → delete.
    keeper.apply_keyring_record("at://a/kr/1", None);

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 2);
    assert!(snaps[1].entries.is_empty());
}

#[test]
fn unwatch_stops_notifications() {
    let mut keeper = WorkspaceKeeper::new();
    let (captured, callback) = capture_snapshots();
    let handle = keeper.install_watcher(callback);

    keeper.unwatch(handle);
    keeper.bootstrap(vec![sample_entry("at://a/kr/1", 1)]);

    let snaps = captured.borrow();
    assert_eq!(
        snaps.len(),
        1,
        "only the initial snapshot — unwatched before bootstrap"
    );
    assert_eq!(keeper.watcher_count(), 0);
}

#[test]
fn uninstall_all_clears_entries_watchers_and_loaded() {
    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap(vec![sample_entry("at://a/kr/1", 1)]);

    let (_captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    keeper.uninstall_all();

    assert!(!keeper.is_loaded());
    assert_eq!(keeper.entry_count(), 0);
    assert_eq!(keeper.watcher_count(), 0);
}

#[test]
fn snapshot_entries_are_sorted_by_uri() {
    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap(vec![
        sample_entry("at://a/kr/c", 1),
        sample_entry("at://a/kr/a", 1),
        sample_entry("at://a/kr/b", 1),
    ]);
    let snap = keeper.snapshot();
    let uris: Vec<&str> = snap
        .entries
        .iter()
        .map(|e| e.workspace_id.as_str())
        .collect();
    assert_eq!(uris, vec!["at://a/kr/a", "at://a/kr/b", "at://a/kr/c"]);
}

#[test]
fn multiple_watchers_all_receive_updates() {
    let mut keeper = WorkspaceKeeper::new();

    let (cap_a, cb_a) = capture_snapshots();
    let (cap_b, cb_b) = capture_snapshots();
    keeper.install_watcher(cb_a);
    keeper.install_watcher(cb_b);

    keeper.bootstrap(vec![sample_entry("at://a/kr/1", 1)]);

    assert_eq!(cap_a.borrow().len(), 2);
    assert_eq!(cap_b.borrow().len(), 2);
}

// ---------------------------------------------------------------------------
// try_build_entry — unwrap-failure semantics (#2)
// ---------------------------------------------------------------------------

fn make_keyring_envelope(
    uri: &str,
    member_did: &str,
    role: crate::records::Role,
    rng: &mut (impl CryptoRng + RngCore),
) -> crate::indexer::types::IndexerEnvelope<crate::records::Keyring> {
    use crate::atproto::AtBytes;
    use crate::crypto::{generate_content_key, wrap_key, WrapContext};
    use crate::records::{EncryptedMetadata, Keyring, KeyringMember, SCHEMA_VERSION};
    use crate::test_utils::TestKeys;

    let keys = TestKeys::generate(member_did);
    let gk = generate_content_key(rng);
    let wrapped = wrap_key(
        &gk,
        &keys.public_keys(),
        member_did,
        &WrapContext::Keyring { uri },
        rng,
    )
    .unwrap();

    crate::indexer::types::IndexerEnvelope {
        uri: uri.to_string(),
        record: Keyring {
            opake_version: SCHEMA_VERSION,
            algo: "aes-256-gcm".into(),
            members: vec![KeyringMember {
                wrapped_key: wrapped,
                role,
            }],
            rotation: 1,
            key_history: Vec::new(),
            // Garbage metadata — tests that exercise unwrap-failure feed
            // wrong keys, so decryption fails before we get here either way.
            encrypted_metadata: EncryptedMetadata {
                ciphertext: AtBytes {
                    encoded: String::new(),
                },
                nonce: AtBytes {
                    encoded: String::new(),
                },
            },
            supersedes: None,
            workspace_id: None,
            created_at: "2026-04-17T00:00:00Z".into(),
            modified_at: None,
        },
        indexed_at: "2026-04-17T00:00:01Z".into(),
        deleted_at: None,
    }
}

/// A wrong private key causes unwrap to fail. The entry must still be
/// returned (name = None), not silently deleted. The workspace stays
/// visible in the sidebar and reconciles on the next SSE event.
#[test]
fn try_build_entry_unwrap_failure_returns_some_without_metadata() {
    use crate::crypto::OsRng;
    use crate::test_utils::TestKeys;

    let mut rng: OsRng = OsRng;
    let envelope = make_keyring_envelope(
        "at://did:plc:alice/at.opake.keyring/abc",
        "did:plc:alice",
        crate::records::Role::Manager,
        &mut rng,
    );

    // Completely different keypair — unwrap will fail.
    let wrong_keys = TestKeys::generate("did:plc:alice");

    let entry = try_build_entry(&envelope, "did:plc:alice", &wrong_keys.private_keys());

    let entry = entry.expect("unwrap failure must return Some, not None");
    assert_eq!(
        entry.workspace_id,
        "at://did:plc:alice/at.opake.keyring/abc"
    );
    assert!(
        entry.name.is_none(),
        "name should be None when unwrap fails"
    );
    assert!(
        entry.description.is_none(),
        "description should be None when unwrap fails"
    );
    assert_eq!(
        entry.my_role.as_deref(),
        Some("manager"),
        "role must be preserved even when unwrap fails"
    );
}

/// Build a *superseded* keyring envelope with real, decryptable metadata.
/// The member wrap and the metadata are both anchored to `genesis_uri`
/// (the stable workspace identity), while the envelope's own URI is
/// `head_uri` — exactly the post-add-member shape where head ≠ genesis.
fn make_superseded_envelope_with_name(
    head_uri: &str,
    genesis_uri: &str,
    member_did: &str,
    name: &str,
    keys: &crate::test_utils::TestKeys,
    rng: &mut (impl CryptoRng + RngCore),
) -> crate::indexer::types::IndexerEnvelope<crate::records::Keyring> {
    use crate::crypto::{encrypt_metadata, generate_content_key, wrap_key, WrapContext};
    use crate::records::{Keyring, KeyringMember, SCHEMA_VERSION};

    let gk = generate_content_key(rng);
    // Wrap anchored to genesis — the way add_member carries members forward.
    let wrapped = wrap_key(
        &gk,
        &keys.public_keys(),
        member_did,
        &WrapContext::Keyring { uri: genesis_uri },
        rng,
    )
    .unwrap();
    let metadata = KeyringMetadata {
        name: name.to_string(),
        description: None,
        icon: None,
    };
    let encrypted_metadata = encrypt_metadata(&gk, &metadata, rng).unwrap();

    crate::indexer::types::IndexerEnvelope {
        uri: head_uri.to_string(),
        record: Keyring {
            opake_version: SCHEMA_VERSION,
            algo: "aes-256-gcm".into(),
            members: vec![KeyringMember {
                wrapped_key: wrapped,
                role: crate::records::Role::Manager,
            }],
            rotation: 0,
            key_history: Vec::new(),
            encrypted_metadata,
            supersedes: Some(genesis_uri.to_string()),
            workspace_id: Some(genesis_uri.to_string()),
            created_at: "2026-04-17T00:00:00Z".into(),
            modified_at: Some("2026-04-17T00:01:00Z".into()),
        },
        indexed_at: "2026-04-17T00:00:01Z".into(),
        deleted_at: None,
    }
}

/// Regression: adding a member supersedes the keyring, so the head URI no
/// longer equals the genesis URI the member wraps are anchored to. Unwrapping
/// against the head (the old bug) fails the AEAD check → name decodes to None
/// → the workspace renders "unnamed". `wrap_anchor` must resolve the genesis
/// URI from the record so the name still decrypts.
#[test]
#[allow(non_snake_case)] // bug__ regression-naming convention
fn bug__superseded_keyring_decrypts_name_via_genesis_anchor() {
    use crate::crypto::OsRng;
    use crate::test_utils::TestKeys;

    let mut rng: OsRng = OsRng;
    let genesis = "at://did:plc:alice/at.opake.keyring/genesis";
    let head = "at://did:plc:alice/at.opake.keyring/head2";
    let keys = TestKeys::generate("did:plc:alice");

    let envelope = make_superseded_envelope_with_name(
        head,
        genesis,
        "did:plc:alice",
        "Shared Space",
        &keys,
        &mut rng,
    );

    let entry = try_build_entry(&envelope, "did:plc:alice", &keys.private_keys())
        .expect("member entry must resolve");

    // Identity is the stable genesis, head tracks the current record.
    assert_eq!(entry.workspace_id, genesis);
    assert_eq!(entry.head_uri, head);
    assert_eq!(
        entry.name.as_deref(),
        Some("Shared Space"),
        "name must decrypt via the genesis anchor, not the head URI"
    );
}

/// Regression: a member-removal supersede delivers an envelope whose URI is
/// the *new head*, while keeper entries are keyed by the genesis workspace
/// id. The wasm dispatch used to key the resulting delete on the envelope
/// URI, so it no-op'd — the removed member's sidebar kept a workspace they
/// could no longer decrypt until the next full bootstrap. The apply must be
/// keyed on `envelope.workspace_id()` (genesis) for the entry to drop.
// spec:workspace-identity § SSE keyring dispatch keys on derived genesis
#[test]
#[allow(non_snake_case)] // bug__ regression-naming convention
fn bug__removal_supersede_drops_workspace_keyed_by_genesis() {
    use crate::crypto::OsRng;
    use crate::test_utils::TestKeys;

    let mut rng: OsRng = OsRng;
    let genesis = "at://did:plc:alice/at.opake.keyring/genesis";
    let head = "at://did:plc:alice/at.opake.keyring/head2";
    let alice = TestKeys::generate("did:plc:alice");

    // Bob's keeper tracks the workspace under its genesis id.
    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap(vec![sample_entry(genesis, 0)]);

    // Removal supersede: fresh head record whose member list holds only
    // alice — exactly what bob receives before the server unsubscribes him.
    let envelope = make_superseded_envelope_with_name(
        head,
        genesis,
        "did:plc:alice",
        "Shared Space",
        &alice,
        &mut rng,
    );

    let bob = TestKeys::generate("did:plc:bob");
    let entry = try_build_entry(&envelope, "did:plc:bob", &bob.private_keys());
    assert!(entry.is_none(), "removed member must not build an entry");

    // The dispatch contract: the apply is keyed on the derived genesis
    // identity, never the envelope (head) URI.
    assert_eq!(envelope.workspace_id().as_str(), genesis);
    keeper.apply_keyring_record(envelope.workspace_id().as_str(), entry);

    assert_eq!(
        keeper.entry_count(),
        0,
        "workspace must drop from the removed member's sidebar"
    );
}

/// Regression: the wasm dispatch used to key a `keyring:delete` on the
/// deleted URI (`keeper.delete(&payload.uri)`). Keeper entries are keyed
/// on genesis, so deleting the genesis *record* of a living workspace —
/// legitimate PDS cleanup; the genesis URI identifies the workspace, not
/// a live record — matched the entry and dropped a workspace whose chain
/// head, keys, and members were all intact. The keeper must act on the
/// indexer-resolved outcome: `unchanged` never touches tracked state,
/// even when the deleted URI equals a tracked key.
// spec:keyring-tombstones § Clients act on the outcome, never on URI matching
#[test]
#[allow(non_snake_case)] // bug__ regression-naming convention
fn bug__genesis_delete_tombstone_drops_living_workspace() {
    use crate::indexer::sse::events::{KeyringDeleteOutcome, SseKeyringDeletePayload};

    let genesis = "at://did:plc:alice/at.opake.keyring/genesis";

    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap(vec![sample_entry(genesis, 0)]);

    let payload = SseKeyringDeletePayload {
        uri: genesis.into(),
        workspace_id: Some(genesis.into()),
        outcome: KeyringDeleteOutcome::Unchanged,
    };
    // The trap the old dispatch fell into: the deleted URI matches the
    // tracked key exactly.
    assert_eq!(payload.uri, keeper.snapshot().entries[0].workspace_id);

    keeper.apply_keyring_delete(&payload);

    assert_eq!(
        keeper.entry_count(),
        1,
        "an unchanged-outcome tombstone must never drop a living workspace"
    );
}

/// `rolled_back` leaves the entry alone — the follow-up `keyring:upsert`
/// of the restored head carries the rebuild.
// spec:keyring-tombstones § Clients act on the outcome, never on URI matching
#[test]
fn rolled_back_delete_defers_to_the_follow_up_upsert() {
    use crate::indexer::sse::events::{KeyringDeleteOutcome, SseKeyringDeletePayload};

    let genesis = "at://did:plc:alice/at.opake.keyring/genesis";
    let head = "at://did:plc:alice/at.opake.keyring/head2";

    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap(vec![sample_entry(genesis, 0)]);

    keeper.apply_keyring_delete(&SseKeyringDeletePayload {
        uri: head.into(),
        workspace_id: Some(genesis.into()),
        outcome: KeyringDeleteOutcome::RolledBack,
    });

    assert_eq!(keeper.entry_count(), 1, "rollback must not drop the entry");
}

/// `torn_down` removes the entry keyed by the payload's workspace
/// identity — matching what the next bootstrap would show, since the
/// indexer has already dropped the workspace's tracked chains.
// spec:keyring-tombstones § Clients act on the outcome, never on URI matching
#[test]
fn torn_down_delete_drops_the_entry_by_workspace_id() {
    use crate::indexer::sse::events::{KeyringDeleteOutcome, SseKeyringDeletePayload};

    let genesis = "at://did:plc:alice/at.opake.keyring/genesis";

    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap(vec![sample_entry(genesis, 0)]);

    keeper.apply_keyring_delete(&SseKeyringDeletePayload {
        uri: genesis.into(),
        workspace_id: Some(genesis.into()),
        outcome: KeyringDeleteOutcome::TornDown,
    });

    assert_eq!(keeper.entry_count(), 0, "torn down workspace must drop");
}

/// The teardown of a *superseded* chain: the last live record is a head, so
/// the delete payload's `uri` is a head URI the keeper has never held — its
/// entries are keyed on genesis. Only the payload's workspace identity finds
/// the entry; a uri-keyed delete silently no-ops and leaves a workspace in
/// the sidebar whose keys are gone. The sibling test above cannot catch that,
/// because on an un-superseded chain the deleted URI *is* the genesis key.
// spec:workspace-identity § SSE keyring dispatch keys on derived genesis
// spec:keyring-tombstones § Clients act on the outcome, never on URI matching
#[test]
fn torn_down_delete_on_a_superseded_chain_drops_the_genesis_keyed_entry() {
    use crate::indexer::sse::events::{KeyringDeleteOutcome, SseKeyringDeletePayload};

    let genesis = "at://did:plc:alice/at.opake.keyring/genesis";
    let head = "at://did:plc:alice/at.opake.keyring/head3";

    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap(vec![sample_entry(genesis, 2)]);

    let payload = SseKeyringDeletePayload {
        uri: head.into(),
        workspace_id: Some(genesis.into()),
        outcome: KeyringDeleteOutcome::TornDown,
    };
    assert_ne!(
        payload.uri,
        keeper.snapshot().entries[0].workspace_id,
        "the deleted head must not match the tracked key, or the test proves nothing"
    );

    keeper.apply_keyring_delete(&payload);

    assert_eq!(
        keeper.entry_count(),
        0,
        "teardown must key on the payload's workspace identity, not the deleted URI"
    );
}

/// DID absent from member list → None → keeper deletes the workspace.
#[test]
fn try_build_entry_non_member_returns_none() {
    use crate::crypto::OsRng;
    use crate::test_utils::TestKeys;

    let mut rng: OsRng = OsRng;
    let envelope = make_keyring_envelope(
        "at://did:plc:alice/at.opake.keyring/abc",
        "did:plc:alice",
        crate::records::Role::Manager,
        &mut rng,
    );

    let bob = TestKeys::generate("did:plc:bob");
    let result = try_build_entry(&envelope, "did:plc:bob", &bob.private_keys());

    assert!(result.is_none(), "non-member DID must return None");
}

// ---------------------------------------------------------------------------
// Distinct unreadable-workspace signal (poison-record-resilience 3.2, 3.5)
// ---------------------------------------------------------------------------

use crate::records::{UnreadableReason, UnreadableRef};

const CORRUPT_KR: &str = "at://did:plc:test/at.opake.keyring/corrupt";

#[test]
fn signal_unreadable_is_distinct_from_absent_and_present() {
    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap(vec![sample_entry("at://ws/readable", 0)]);

    keeper.signal_unreadable(CORRUPT_KR, UnreadableReason::Corrupt);

    let snap = keeper.snapshot();
    // Readable workspace present in entries; corrupt one only in `unreadable`.
    assert_eq!(snap.entries.len(), 1);
    assert_eq!(snap.unreadable.len(), 1);
    assert_eq!(snap.unreadable[0].uri, CORRUPT_KR);
    assert_eq!(snap.unreadable[0].reason, UnreadableReason::Corrupt);
    assert!(!snap.entries.iter().any(|e| e.workspace_id == CORRUPT_KR));
}

#[test]
fn bootstrap_with_signals_carries_unreadable() {
    let mut keeper = WorkspaceKeeper::new();
    keeper.bootstrap_with_signals(
        vec![sample_entry("at://ws/a", 0)],
        &[UnreadableRef::needs_newer_client(Some(CORRUPT_KR.into()))],
    );
    let snap = keeper.snapshot();
    assert_eq!(snap.entries.len(), 1);
    assert_eq!(snap.unreadable.len(), 1);
    assert_eq!(snap.unreadable[0].reason, UnreadableReason::NeedsNewerClient);
}

// A corrupt keyring met via SSE upsert signals identically to bootstrap, and a
// later readable record for the same workspace clears the signal.
// spec:record-validity § SSE delivery matches snapshot delivery
#[test]
fn readable_upsert_clears_unreadable_signal() {
    let mut keeper = WorkspaceKeeper::new();
    keeper.signal_unreadable(CORRUPT_KR, UnreadableReason::Corrupt);
    assert_eq!(keeper.unreadable_count(), 1);

    // A readable entry keyed by the same URI (genesis-shaped) supersedes it.
    keeper.upsert(sample_entry(CORRUPT_KR, 1));

    assert_eq!(keeper.unreadable_count(), 0);
    let snap = keeper.snapshot();
    assert!(snap.unreadable.is_empty());
    assert!(snap.entries.iter().any(|e| e.workspace_id == CORRUPT_KR));
}

#[test]
fn signal_unreadable_is_idempotent() {
    let (captured, callback) = capture_snapshots();
    let mut keeper = WorkspaceKeeper::new();
    keeper.install_watcher(callback);
    let before = captured.borrow().len();

    keeper.signal_unreadable(CORRUPT_KR, UnreadableReason::Corrupt);
    keeper.signal_unreadable(CORRUPT_KR, UnreadableReason::Corrupt);

    // First signal fires; the identical repeat does not.
    assert_eq!(captured.borrow().len(), before + 1);
}
