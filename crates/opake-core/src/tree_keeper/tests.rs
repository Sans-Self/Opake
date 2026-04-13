use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::crypto::ContentKey;
use crate::directories::tests::{dummy_directory_with_entries, test_keypair, TEST_DID};
use crate::sse::events::{SseDeletePayload, SseDirectoryRecord, SseEvent};

const ROOT_URI: &str = "at://did:plc:test/app.opake.directory/self";
const DIR_PHOTOS_URI: &str = "at://did:plc:test/app.opake.directory/photos";
const DOC_BEACH_URI: &str = "at://did:plc:test/app.opake.document/beach";

// -- Test helpers --

/// A callback sink that records every notification for later assertion.
#[derive(Default, Clone)]
struct RecordingSink {
    events: Rc<RefCell<Vec<RecordingEvent>>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // root_uri captured for future assertion use
enum RecordingEvent {
    /// Snapshot delivered. Captures the root URI (enough for the POC
    /// tests; full tree comparison would be overkill).
    Snapshot { root_uri: Option<String> },
    /// Watcher was auto-closed due to the watched directory being deleted.
    Gone,
}

impl RecordingSink {
    fn new() -> Self {
        Self::default()
    }

    fn callback(&self) -> WatcherCallback {
        let events = Rc::clone(&self.events);
        Box::new(move |tree: Option<&DirectoryTree>| match tree {
            Some(t) => events.borrow_mut().push(RecordingEvent::Snapshot {
                root_uri: t.root_uri().map(str::to_owned),
            }),
            None => events.borrow_mut().push(RecordingEvent::Gone),
        })
    }

    fn count(&self) -> usize {
        self.events.borrow().len()
    }

    fn was_closed(&self) -> bool {
        self.events
            .borrow()
            .iter()
            .any(|e| matches!(e, RecordingEvent::Gone))
    }
}

fn cabinet_keeper() -> TreeKeeper {
    let mut keeper = TreeKeeper::new(TEST_DID);
    let (_, private_key) = test_keypair();
    let tree = DirectoryTree::from_records(std::iter::empty());
    keeper.install_cabinet_tree(tree, private_key);
    keeper
}

fn sse_dir_upsert(uri: &str, name: &str, entries: Vec<String>) -> SseEvent {
    let dir = dummy_directory_with_entries(name, entries);
    SseEvent::DirectoryUpsert(SseDirectoryRecord {
        directory_uri: uri.into(),
        owner_did: TEST_DID.into(),
        entries: dir.entries.clone(),
        encrypted_metadata: Some(serde_json::to_value(&dir.encrypted_metadata).unwrap()),
        key_wrapping: Some(serde_json::to_value(&dir.key_wrapping).unwrap()),
        keyring_uri: None,
        deleted_at: None,
        indexed_at: None,
    })
}

fn sse_dir_delete(uri: &str) -> SseEvent {
    SseEvent::DirectoryDelete(SseDeletePayload {
        uri: None,
        directory_uri: Some(uri.into()),
        document_uri: None,
    })
}

// -- Tests --

#[test]
fn watcher_fires_once_on_install() {
    let mut keeper = cabinet_keeper();
    let sink = RecordingSink::new();

    keeper.watch_cabinet(ROOT_URI.into(), sink.callback());

    // Eager first snapshot: 1 call at registration time.
    assert_eq!(sink.count(), 1);
}

#[test]
fn watcher_not_fired_before_install() {
    let mut keeper = TreeKeeper::new(TEST_DID);
    let sink = RecordingSink::new();

    // No tree installed yet.
    keeper.watch_cabinet(ROOT_URI.into(), sink.callback());
    assert_eq!(sink.count(), 0);
}

#[test]
fn upsert_event_fires_watchers() {
    let mut keeper = cabinet_keeper();
    let sink = RecordingSink::new();
    keeper.watch_cabinet(ROOT_URI.into(), sink.callback());

    // Drop the eager first-snapshot call to isolate the event fire.
    let initial_count = sink.count();

    keeper
        .apply_event(&sse_dir_upsert(ROOT_URI, "/", vec![]))
        .unwrap();

    assert_eq!(sink.count(), initial_count + 1);
}

#[test]
fn cold_start_drops_events_for_missing_context() {
    let mut keeper = TreeKeeper::new(TEST_DID);
    // No tree installed.

    // Should not panic — just drops the event silently.
    keeper
        .apply_event(&sse_dir_upsert(ROOT_URI, "/", vec![]))
        .unwrap();
}

#[test]
fn watch_workspace_scoped_events_only() {
    let mut keeper = TreeKeeper::new(TEST_DID);
    let (_, private_key) = test_keypair();
    keeper.install_cabinet_tree(DirectoryTree::from_records(std::iter::empty()), private_key);

    let cabinet_sink = RecordingSink::new();
    let ws_sink = RecordingSink::new();

    keeper.watch_cabinet(ROOT_URI.into(), cabinet_sink.callback());
    // Workspace tree isn't installed — ws_sink shouldn't receive events
    // (including the eager first snapshot, since we never install).
    keeper.watch_workspace(
        "at://did:plc:test/app.opake.keyring/ws1".into(),
        "at://did:plc:test/app.opake.directory/ws-root".into(),
        ws_sink.callback(),
    );
    assert_eq!(ws_sink.count(), 0);

    let cabinet_start = cabinet_sink.count();
    keeper
        .apply_event(&sse_dir_upsert(ROOT_URI, "/", vec![]))
        .unwrap();

    assert_eq!(cabinet_sink.count(), cabinet_start + 1);
    assert_eq!(ws_sink.count(), 0); // unchanged — event was cabinet-scoped
}

#[test]
fn deletion_of_watched_directory_fires_gone_and_auto_closes() {
    let mut keeper = cabinet_keeper();

    // Insert the directory first so it exists in the tree.
    keeper
        .apply_event(&sse_dir_upsert(
            DIR_PHOTOS_URI,
            "Photos",
            vec![DOC_BEACH_URI.into()],
        ))
        .unwrap();

    let sink = RecordingSink::new();
    keeper.watch_cabinet(DIR_PHOTOS_URI.into(), sink.callback());

    // At registration the watcher gets one snapshot.
    let start = sink.count();

    // Delete the watched directory.
    keeper.apply_event(&sse_dir_delete(DIR_PHOTOS_URI)).unwrap();

    // The watcher should have received a Gone marker, and been removed.
    assert!(sink.was_closed());
    assert_eq!(sink.count(), start + 1);
    assert_eq!(keeper.watcher_count(), 0);

    // A subsequent event should NOT fire the closed watcher.
    keeper
        .apply_event(&sse_dir_upsert(DIR_PHOTOS_URI, "Photos", vec![]))
        .unwrap();
    assert_eq!(sink.count(), start + 1);
}

#[test]
fn unwatch_removes_watcher() {
    let mut keeper = cabinet_keeper();
    let sink = RecordingSink::new();

    let handle = keeper.watch_cabinet(ROOT_URI.into(), sink.callback());
    assert_eq!(keeper.watcher_count(), 1);

    keeper.unwatch(handle);
    assert_eq!(keeper.watcher_count(), 0);

    // Events after unwatch shouldn't fire the callback.
    let count_before = sink.count();
    keeper
        .apply_event(&sse_dir_upsert(ROOT_URI, "/", vec![]))
        .unwrap();
    assert_eq!(sink.count(), count_before);
}

#[test]
fn reconnect_event_fires_all_watchers() {
    let mut keeper = cabinet_keeper();
    let a = RecordingSink::new();
    let b = RecordingSink::new();
    keeper.watch_cabinet(ROOT_URI.into(), a.callback());
    keeper.watch_cabinet(DIR_PHOTOS_URI.into(), b.callback());

    let a_start = a.count();
    let b_start = b.count();

    keeper.apply_event(&SseEvent::Reconnect).unwrap();

    assert_eq!(a.count(), a_start + 1);
    assert_eq!(b.count(), b_start + 1);
}

#[test]
fn uninstall_cabinet_drops_cabinet_watchers() {
    let mut keeper = cabinet_keeper();
    let cabinet_sink = RecordingSink::new();
    keeper.watch_cabinet(ROOT_URI.into(), cabinet_sink.callback());
    assert_eq!(keeper.watcher_count(), 1);

    keeper.uninstall_cabinet();
    assert_eq!(keeper.watcher_count(), 0);
    assert!(keeper.cabinet_tree().is_none());
}

#[test]
fn uninstall_all_drains_every_scope() {
    // Install cabinet + two workspace trees, attach watchers to each,
    // then call uninstall_all. This is the shape of the "account
    // switch" path: all scopes empty, all watchers removed. Drop
    // semantics on `ContentKey` (ZeroizeOnDrop) are covered in the
    // `crypto` module's own tests — here we only verify state drain.
    let mut keeper = cabinet_keeper();
    let ws_a = "at://did:plc:test/app.opake.keyring/a".to_string();
    let ws_b = "at://did:plc:test/app.opake.keyring/b".to_string();

    let group_key_a = ContentKey([0u8; 32]);
    let group_key_b = ContentKey([1u8; 32]);
    keeper.install_workspace_tree(
        ws_a.clone(),
        DirectoryTree::from_records(std::iter::empty()),
        group_key_a,
    );
    keeper.install_workspace_tree(
        ws_b.clone(),
        DirectoryTree::from_records(std::iter::empty()),
        group_key_b,
    );

    let cabinet_sink = RecordingSink::new();
    let ws_a_sink = RecordingSink::new();
    let ws_b_sink = RecordingSink::new();
    keeper.watch_cabinet(ROOT_URI.into(), cabinet_sink.callback());
    keeper.watch_workspace(
        ws_a.clone(),
        "at://did:plc:test/app.opake.directory/a-root".into(),
        ws_a_sink.callback(),
    );
    keeper.watch_workspace(
        ws_b.clone(),
        "at://did:plc:test/app.opake.directory/b-root".into(),
        ws_b_sink.callback(),
    );

    assert_eq!(keeper.watcher_count(), 3);
    assert!(keeper.cabinet_tree().is_some());
    assert!(keeper.workspace_tree(&ws_a).is_some());
    assert!(keeper.workspace_tree(&ws_b).is_some());

    keeper.uninstall_all();

    assert_eq!(keeper.watcher_count(), 0);
    assert!(keeper.cabinet_tree().is_none());
    assert!(keeper.workspace_tree(&ws_a).is_none());
    assert!(keeper.workspace_tree(&ws_b).is_none());

    // Subsequent events are silently dropped (cold-start semantics).
    keeper
        .apply_event(&sse_dir_upsert(ROOT_URI, "/", vec![]))
        .unwrap();
    // No watcher should have fired from that event — all closed.
    assert_eq!(keeper.watcher_count(), 0);
}

fn sse_doc_upsert(uri: &str, keyring_uri: Option<&str>) -> SseEvent {
    use crate::sse::events::SseDocumentRecord;
    SseEvent::DocumentUpsert(SseDocumentRecord {
        document_uri: uri.into(),
        owner_did: TEST_DID.into(),
        encrypted_metadata: None,
        encryption: None,
        blob_ref: None,
        keyring_uri: keyring_uri.map(str::to_owned),
        rotation: None,
        deleted_at: None,
        indexed_at: None,
    })
}

fn sse_doc_delete(uri: &str) -> SseEvent {
    use crate::sse::events::SseDeletePayload;
    SseEvent::DocumentDelete(SseDeletePayload {
        uri: None,
        directory_uri: None,
        document_uri: Some(uri.into()),
    })
}

#[test]
fn document_upsert_fires_cabinet_watchers_when_keyring_uri_is_none() {
    // A cabinet document's metadata changed — the tree structure is
    // untouched but consumers need to refetch the document's encrypted
    // metadata. TreeKeeper fires cabinet-scoped watchers with the
    // current (unchanged) tree and lets the consumer's reload cycle
    // pick up the delta.
    let mut keeper = cabinet_keeper();
    let sink = RecordingSink::new();
    keeper.watch_cabinet(ROOT_URI.into(), sink.callback());
    let start = sink.count();

    keeper
        .apply_event(&sse_doc_upsert(DOC_BEACH_URI, None))
        .unwrap();

    assert_eq!(sink.count(), start + 1);
}

#[test]
fn document_upsert_with_keyring_fires_only_that_workspace_watcher() {
    // A workspace document upsert should fire watchers for that
    // specific workspace — not cabinet, not other workspaces.
    let mut keeper = cabinet_keeper();
    let ws_a = "at://did:plc:test/app.opake.keyring/a".to_string();
    let ws_b = "at://did:plc:test/app.opake.keyring/b".to_string();
    keeper.install_workspace_tree(
        ws_a.clone(),
        DirectoryTree::from_records(std::iter::empty()),
        ContentKey([0u8; 32]),
    );
    keeper.install_workspace_tree(
        ws_b.clone(),
        DirectoryTree::from_records(std::iter::empty()),
        ContentKey([1u8; 32]),
    );

    let cabinet_sink = RecordingSink::new();
    let ws_a_sink = RecordingSink::new();
    let ws_b_sink = RecordingSink::new();
    keeper.watch_cabinet(ROOT_URI.into(), cabinet_sink.callback());
    keeper.watch_workspace(
        ws_a.clone(),
        "at://did:plc:test/app.opake.directory/a-root".into(),
        ws_a_sink.callback(),
    );
    keeper.watch_workspace(
        ws_b.clone(),
        "at://did:plc:test/app.opake.directory/b-root".into(),
        ws_b_sink.callback(),
    );

    let [cab0, a0, b0] = [cabinet_sink.count(), ws_a_sink.count(), ws_b_sink.count()];

    keeper
        .apply_event(&sse_doc_upsert(DOC_BEACH_URI, Some(&ws_a)))
        .unwrap();

    assert_eq!(cabinet_sink.count(), cab0);
    assert_eq!(ws_a_sink.count(), a0 + 1);
    assert_eq!(ws_b_sink.count(), b0);
}

#[test]
fn document_upsert_with_empty_string_keyring_routes_to_cabinet() {
    // `SseDocumentRecord.keyring_uri` is `Option<String>`. Upstream
    // serde deserialization can land an empty string in the field
    // where None was intended. Routing that to `Workspace("")`
    // would match no installed tree and silently drop the event.
    // Treat empty string as None → cabinet.
    let mut keeper = cabinet_keeper();
    let sink = RecordingSink::new();
    keeper.watch_cabinet(ROOT_URI.into(), sink.callback());
    let start = sink.count();

    keeper
        .apply_event(&sse_doc_upsert(DOC_BEACH_URI, Some("")))
        .unwrap();

    assert_eq!(sink.count(), start + 1);
}

#[test]
fn document_delete_fires_all_watchers_defensively() {
    // `remove_document` in the core client is not atomic —
    // `delete_record` can succeed while the subsequent
    // `remove_entry` fails, leaving the parent directory listing
    // a now-dead URI with no companion `DirectoryUpsert` to come.
    // Firing watchers on the delete event gives the UI a
    // defensive refresh signal so the stale entry gets culled
    // whenever the consumer's reload cycle next runs.
    let mut keeper = cabinet_keeper();
    let sink = RecordingSink::new();
    keeper.watch_cabinet(ROOT_URI.into(), sink.callback());
    let start = sink.count();

    keeper.apply_event(&sse_doc_delete(DOC_BEACH_URI)).unwrap();

    assert_eq!(sink.count(), start + 1);
}

#[test]
fn multiple_watchers_same_directory_all_fire() {
    let mut keeper = cabinet_keeper();
    let a = RecordingSink::new();
    let b = RecordingSink::new();
    let c = RecordingSink::new();

    keeper.watch_cabinet(ROOT_URI.into(), a.callback());
    keeper.watch_cabinet(ROOT_URI.into(), b.callback());
    keeper.watch_cabinet(ROOT_URI.into(), c.callback());

    let [a0, b0, c0] = [a.count(), b.count(), c.count()];

    keeper
        .apply_event(&sse_dir_upsert(ROOT_URI, "/", vec![]))
        .unwrap();

    assert_eq!(a.count(), a0 + 1);
    assert_eq!(b.count(), b0 + 1);
    assert_eq!(c.count(), c0 + 1);
}
