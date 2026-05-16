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
    let uris: Vec<&str> = snap.entries.iter().map(|e| e.workspace_id.as_str()).collect();
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
                ciphertext: AtBytes { encoded: String::new() },
                nonce: AtBytes { encoded: String::new() },
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
        "at://did:plc:alice/app.opake.keyring/abc",
        "did:plc:alice",
        crate::records::Role::Manager,
        &mut rng,
    );

    // Completely different keypair — unwrap will fail.
    let wrong_keys = TestKeys::generate("did:plc:alice");

    let entry = try_build_entry(&envelope, "did:plc:alice", &wrong_keys.private_keys());

    let entry = entry.expect("unwrap failure must return Some, not None");
    assert_eq!(entry.workspace_id, "at://did:plc:alice/app.opake.keyring/abc");
    assert!(entry.name.is_none(), "name should be None when unwrap fails");
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

/// DID absent from member list → None → keeper deletes the workspace.
#[test]
fn try_build_entry_non_member_returns_none() {
    use crate::crypto::OsRng;
    use crate::test_utils::TestKeys;

    let mut rng: OsRng = OsRng;
    let envelope = make_keyring_envelope(
        "at://did:plc:alice/app.opake.keyring/abc",
        "did:plc:alice",
        crate::records::Role::Manager,
        &mut rng,
    );

    let bob = TestKeys::generate("did:plc:bob");
    let result = try_build_entry(&envelope, "did:plc:bob", &bob.private_keys());

    assert!(result.is_none(), "non-member DID must return None");
}
