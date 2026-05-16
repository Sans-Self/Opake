// InboxKeeper unit tests.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;

fn sample_entry(uri: &str, owner: &str) -> InboxEntry {
    InboxEntry {
        uri: uri.to_string(),
        author_did: owner.to_string(),
        document_uri: format!("at://{owner}/app.opake.document/doc1"),
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
        sample_entry("at://a/app.opake.grant/g1", "did:plc:alice"),
        sample_entry("at://a/app.opake.grant/g2", "did:plc:alice"),
    ]);

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 2);
    assert!(snaps[1].loaded);
    assert_eq!(snaps[1].entries.len(), 2);
}

#[test]
fn upsert_with_identical_entry_does_not_refire() {
    let mut keeper = InboxKeeper::new();
    let entry = sample_entry("at://a/app.opake.grant/g1", "did:plc:alice");
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
        "at://a/app.opake.grant/g1",
        "did:plc:alice",
    )]);

    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    let mut updated = sample_entry("at://a/app.opake.grant/g1", "did:plc:alice");
    updated.created_at = "2026-04-18T00:00:00Z".to_string();
    keeper.upsert(updated);

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 2, "install fire + update fire");
}

#[test]
fn delete_removes_and_fires() {
    let mut keeper = InboxKeeper::new();
    keeper.bootstrap(vec![sample_entry(
        "at://a/app.opake.grant/g1",
        "did:plc:alice",
    )]);

    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    keeper.delete("at://a/app.opake.grant/g1");

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 2);
    assert!(snaps[1].entries.is_empty());
}

#[test]
fn delete_of_unknown_uri_does_not_fire() {
    let mut keeper = InboxKeeper::new();
    keeper.bootstrap(vec![sample_entry(
        "at://a/app.opake.grant/g1",
        "did:plc:alice",
    )]);

    let (captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    keeper.delete("at://a/app.opake.grant/g-unknown");

    let snaps = captured.borrow();
    assert_eq!(snaps.len(), 1, "no fire on no-op delete");
}

#[test]
fn uninstall_all_drains_and_resets() {
    let mut keeper = InboxKeeper::new();
    keeper.bootstrap(vec![sample_entry(
        "at://a/app.opake.grant/g1",
        "did:plc:alice",
    )]);

    let (_captured, callback) = capture_snapshots();
    keeper.install_watcher(callback);

    keeper.uninstall_all();

    assert!(!keeper.is_loaded());
    assert_eq!(keeper.entry_count(), 0);
    assert_eq!(keeper.watcher_count(), 0);
}

#[test]
fn try_build_entry_filters_non_recipient() {
    let record = crate::indexer::sse::events::SseGrantRecord {
        uri: "at://a/app.opake.grant/g1".to_string(),
        author_did: "did:plc:alice".to_string(),
        recipient_did: Some("did:plc:bob".to_string()),
        document_uri: "at://a/app.opake.document/d1".to_string(),
        created_at: Some("2026-04-17T00:00:00Z".to_string()),
    };

    // Caller is carol — shouldn't see bob's grant.
    assert!(try_build_entry_from_sse_record(&record, "did:plc:carol").is_none());
    // Caller is bob — should see it.
    assert!(try_build_entry_from_sse_record(&record, "did:plc:bob").is_some());
}

#[test]
fn try_build_entry_defaults_created_at_when_absent() {
    let record = crate::indexer::sse::events::SseGrantRecord {
        uri: "at://a/app.opake.grant/g1".to_string(),
        author_did: "did:plc:alice".to_string(),
        recipient_did: None,
        document_uri: "at://a/app.opake.document/d1".to_string(),
        created_at: None,
    };

    let entry = try_build_entry_from_sse_record(&record, "did:plc:bob").unwrap();
    assert_eq!(entry.created_at, "");
}
