// InboxKeeper unit tests.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;

fn sample_entry(uri: &str, owner: &str) -> InboxEntry {
    InboxEntry {
        uri: uri.to_string(),
        author_did: owner.to_string(),
        document_uri: format!("at://{owner}/at.opake.document/doc1"),
        created_at: "2026-04-17T00:00:00Z".to_string(),
    }
}

fn capture_snapshots() -> (Rc<RefCell<Vec<InboxSnapshot>>>, InboxWatcherCallback) {
    let captured: Rc<RefCell<Vec<InboxSnapshot>>> = Rc::new(RefCell::new(Vec::new()));
    let captured_clone = Rc::clone(&captured);
    let callback: InboxWatcherCallback =
        Box::new(move |snap: &InboxSnapshot| captured_clone.borrow_mut().push(snap.clone()));
    (captured, callback)
}

#[test]
fn new_keeper_is_empty_and_unloaded() {
    let keeper = InboxKeeper::new();
    assert!(!keeper.is_loaded());
    assert_eq!(keeper.entry_count(), 0);
    assert_eq!(keeper.watcher_count(), 0);
}

#[test]
fn install_watcher_fires_initial_snapshot() {
    let mut keeper = InboxKeeper::new();
    let (captured, callback) = capture_snapshots();
    let _h = keeper.install_watcher(callback);

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 1);
    assert!(!snaps[0].loaded);
    assert!(snaps[0].entries.is_empty());
}

#[test]
fn bootstrap_sets_loaded_and_notifies_watcher() {
    let mut keeper = InboxKeeper::new();
    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    keeper.bootstrap(vec![
        sample_entry("at://a/at.opake.grant/g1", "did:plc:alice"),
        sample_entry("at://a/at.opake.grant/g2", "did:plc:alice"),
    ]);

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 2);
    assert!(snaps[1].loaded);
    assert_eq!(snaps[1].entries.len(), 2);
}

#[test]
fn upsert_with_identical_entry_does_not_refire() {
    let mut keeper = InboxKeeper::new();
    let entry = sample_entry("at://a/at.opake.grant/g1", "did:plc:alice");
    keeper.bootstrap(vec![entry.clone()]);

    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    // Idempotent upsert — same fields, no change.
    keeper.upsert(entry.clone());

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 1, "no refire on identical upsert");
}

#[test]
fn upsert_refires_on_change() {
    let mut keeper = InboxKeeper::new();
    keeper.bootstrap(vec![sample_entry(
        "at://a/at.opake.grant/g1",
        "did:plc:alice",
    )]);

    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    let mut updated = sample_entry("at://a/at.opake.grant/g1", "did:plc:alice");
    updated.created_at = "2026-04-18T00:00:00Z".to_string();
    keeper.upsert(updated);

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 2, "install fire + update fire");
}

#[test]
fn delete_removes_and_fires() {
    let mut keeper = InboxKeeper::new();
    keeper.bootstrap(vec![sample_entry(
        "at://a/at.opake.grant/g1",
        "did:plc:alice",
    )]);

    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    keeper.delete("at://a/at.opake.grant/g1");

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 2);
    assert!(snaps[1].entries.is_empty());
}

#[test]
fn delete_of_unknown_uri_does_not_fire() {
    let mut keeper = InboxKeeper::new();
    keeper.bootstrap(vec![sample_entry(
        "at://a/at.opake.grant/g1",
        "did:plc:alice",
    )]);

    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    keeper.delete("at://a/at.opake.grant/g-unknown");

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 1, "no fire on no-op delete");
}

#[test]
fn uninstall_all_drains_and_resets() {
    let mut keeper = InboxKeeper::new();
    keeper.bootstrap(vec![sample_entry(
        "at://a/at.opake.grant/g1",
        "did:plc:alice",
    )]);

    let (_captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    keeper.uninstall_all();

    assert!(!keeper.is_loaded());
    assert_eq!(keeper.entry_count(), 0);
    assert_eq!(keeper.watcher_count(), 0);
}

fn fixture_envelope(
    uri: &str,
    recipient: &str,
    document: &str,
    created_at: &str,
) -> crate::indexer::types::IndexerEnvelope<crate::records::Grant> {
    use crate::atproto::AtBytes;
    use crate::records::{EncryptedMetadata, Grant, WrappedKey, SCHEMA_VERSION};

    crate::indexer::types::IndexerEnvelope {
        uri: uri.to_string(),
        record: Grant {
            opake_version: SCHEMA_VERSION,
            document: document.to_string(),
            recipient: recipient.to_string(),
            wrapped_key: WrappedKey {
                did: recipient.to_string(),
                ciphertext: AtBytes {
                    encoded: String::new(),
                },
                algo: "x25519-mlkem768-hkdf-a256kw-v2".to_string(),
            },
            encrypted_metadata: EncryptedMetadata {
                ciphertext: AtBytes {
                    encoded: String::new(),
                },
                nonce: AtBytes {
                    encoded: String::new(),
                },
            },
            created_at: created_at.to_string(),
        },
        indexed_at: "2026-04-17T00:00:01Z".into(),
        deleted_at: None,
    }
}

#[test]
fn try_build_entry_filters_non_recipient() {
    let envelope = fixture_envelope(
        "at://did:plc:alice/at.opake.grant/g1",
        "did:plc:bob",
        "at://did:plc:alice/at.opake.document/d1",
        "2026-04-17T00:00:00Z",
    );

    // Caller is carol — shouldn't see bob's grant.
    assert!(try_build_entry_from_envelope(&envelope, "did:plc:carol").is_none());
    // Caller is bob — should see it.
    assert!(try_build_entry_from_envelope(&envelope, "did:plc:bob").is_some());
}

#[test]
fn try_build_entry_pulls_author_did_from_uri() {
    let envelope = fixture_envelope(
        "at://did:plc:alice/at.opake.grant/g1",
        "did:plc:bob",
        "at://did:plc:alice/at.opake.document/d1",
        "2026-04-17T00:00:00Z",
    );

    let entry = try_build_entry_from_envelope(&envelope, "did:plc:bob").unwrap();
    assert_eq!(entry.author_did, "did:plc:alice");
    assert_eq!(
        entry.document_uri,
        "at://did:plc:alice/at.opake.document/d1"
    );
    assert_eq!(entry.created_at, "2026-04-17T00:00:00Z");
}

// ---------------------------------------------------------------------------
// Distinct unreadable-grant signal (poison-record-resilience 3.2)
// ---------------------------------------------------------------------------

use crate::records::{UnreadableReason, UnreadableRef};

const CORRUPT_GRANT: &str = "at://did:plc:author/at.opake.grant/corrupt";

#[test]
fn signal_unreadable_grant_is_distinct_from_entries() {
    let mut keeper = InboxKeeper::new();
    keeper.bootstrap(vec![sample_entry("at://g/readable", "did:plc:author")]);
    keeper.signal_unreadable(CORRUPT_GRANT, UnreadableReason::Corrupt);

    let snap = keeper.snapshot();
    assert_eq!(snap.entries.len(), 1);
    assert_eq!(snap.unreadable.len(), 1);
    assert_eq!(snap.unreadable[0].uri, CORRUPT_GRANT);
    assert!(!snap.entries.iter().any(|e| e.uri == CORRUPT_GRANT));
}

#[test]
fn readable_grant_upsert_clears_unreadable_signal() {
    let mut keeper = InboxKeeper::new();
    keeper.signal_unreadable(CORRUPT_GRANT, UnreadableReason::Corrupt);
    assert_eq!(keeper.unreadable_count(), 1);

    keeper.upsert(sample_entry(CORRUPT_GRANT, "did:plc:author"));
    assert_eq!(keeper.unreadable_count(), 0);
    assert!(keeper.snapshot().unreadable.is_empty());
}

#[test]
fn bootstrap_with_signals_carries_unreadable_grants() {
    let mut keeper = InboxKeeper::new();
    keeper.bootstrap_with_signals(
        vec![sample_entry("at://g/a", "did:plc:author")],
        &[UnreadableRef::corrupt(Some(CORRUPT_GRANT.into()))],
    );
    let snap = keeper.snapshot();
    assert_eq!(snap.entries.len(), 1);
    assert_eq!(snap.unreadable.len(), 1);
}
