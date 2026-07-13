use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::crypto::ContentKey;
use crate::directories::tests::{dummy_directory_with_entries, test_keypair, TEST_DID};
use crate::indexer::sse::events::{SseDeletePayload, SseEvent};
use crate::indexer::types::IndexerEnvelope;

const ROOT_URI: &str = "at://did:plc:test/app.opake.directory/self";
const DIR_PHOTOS_URI: &str = "at://did:plc:test/app.opake.directory/photos";
const DOC_BEACH_URI: &str = "at://did:plc:test/app.opake.document/beach";
const DIR_PARENT_URI: &str = "at://did:plc:test/app.opake.directory/parent";
const DIR_CHILD_URI: &str = "at://did:plc:test/app.opake.directory/child";

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
    let kp = test_keypair();
    let tree = DirectoryTree::from_records(std::iter::empty());
    keeper.install_cabinet_tree(tree, kp.x25519_private, kp.ml_kem_private);
    keeper
}

/// Install a workspace tree with the deterministic test keypair as the
/// caller's private keys. Convenience for the many tests that don't
/// exercise rotation adoption and just need a tree present.
fn install_ws(
    keeper: &mut TreeKeeper,
    keyring_uri: &str,
    tree: DirectoryTree,
    group_key: ContentKey,
    rotation: u64,
) {
    let kp = test_keypair();
    keeper.install_workspace_tree(
        keyring_uri.into(),
        tree,
        group_key,
        rotation,
        Vec::new(),
        kp.x25519_private,
        kp.ml_kem_private,
    );
}

fn sse_dir_upsert(uri: &str, name: &str, entries: Vec<String>) -> SseEvent {
    let dir = dummy_directory_with_entries(name, entries);
    SseEvent::DirectoryUpsert(IndexerEnvelope {
        uri: uri.into(),
        record: dir,
        indexed_at: "2026-04-17T00:00:00Z".into(),
        deleted_at: None,
    })
}

fn sse_dir_delete(uri: &str) -> SseEvent {
    SseEvent::DirectoryDelete(SseDeletePayload { uri: uri.into() })
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
    let kp = test_keypair();
    keeper.install_cabinet_tree(
        DirectoryTree::from_records(std::iter::empty()),
        kp.x25519_private,
        kp.ml_kem_private,
    );

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
fn deletion_of_ancestor_closes_descendant_watcher() {
    // root(self) → parent → child. Watching the child, we delete the
    // parent. The child's parent record is now gone, so the child is
    // unreachable from the root — its watcher must receive Gone and
    // auto-close rather than linger as a zombie subscribed to an orphan.
    let mut keeper = TreeKeeper::new(TEST_DID);
    let kp = test_keypair();
    let tree = DirectoryTree::from_records(vec![
        (
            ROOT_URI.to_string(),
            dummy_directory_with_entries("/", vec![DIR_PARENT_URI.into()]),
        ),
        (
            DIR_PARENT_URI.to_string(),
            dummy_directory_with_entries("Parent", vec![DIR_CHILD_URI.into()]),
        ),
        (
            DIR_CHILD_URI.to_string(),
            dummy_directory_with_entries("Child", vec![]),
        ),
    ]);
    // The "self"-rkey record is detected as the root, so reachability is
    // measurable.
    assert_eq!(tree.root_uri(), Some(ROOT_URI));
    keeper.install_cabinet_tree(tree, kp.x25519_private, kp.ml_kem_private);

    let child_sink = RecordingSink::new();
    keeper.watch_cabinet(DIR_CHILD_URI.into(), child_sink.callback());
    let root_sink = RecordingSink::new();
    keeper.watch_cabinet(ROOT_URI.into(), root_sink.callback());

    keeper.apply_event(&sse_dir_delete(DIR_PARENT_URI)).unwrap();

    // Child watcher: orphaned by the ancestor delete → Gone + removed.
    assert!(child_sink.was_closed());
    // Root watcher: still reachable → not closed, still registered.
    assert!(!root_sink.was_closed());
    assert_eq!(keeper.watcher_count(), 1);
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
    install_ws(
        &mut keeper,
        &ws_a,
        DirectoryTree::from_records(std::iter::empty()),
        group_key_a,
        0,
    );
    install_ws(
        &mut keeper,
        &ws_b,
        DirectoryTree::from_records(std::iter::empty()),
        group_key_b,
        0,
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
    use crate::atproto::{AtBytes, BlobRef, CidLink};
    use crate::records::{
        DirectEncryption, Document, EncryptedMetadata, Encryption, EncryptionEnvelope,
        SCHEMA_VERSION,
    };

    // Minimal document fixture — tests only care about `workspace_id`. The
    // blob and crypto fields are ceremony to satisfy the struct shape.
    let doc = Document {
        opake_version: SCHEMA_VERSION,
        blob: BlobRef {
            blob_type: "blob".into(),
            reference: CidLink {
                cid: "bafyfake".into(),
            },
            mime_type: "application/octet-stream".into(),
            size: 0,
        },
        encryption: Encryption::Direct(DirectEncryption {
            envelope: EncryptionEnvelope {
                algo: "aes-256-gcm".into(),
                nonce: AtBytes {
                    encoded: String::new(),
                },
                keys: Vec::new(),
            },
        }),
        encrypted_metadata: EncryptedMetadata {
            ciphertext: AtBytes {
                encoded: String::new(),
            },
            nonce: AtBytes {
                encoded: String::new(),
            },
        },
        supersedes: None,
        workspace_id: keyring_uri.map(str::to_owned),
        created_at: "2026-04-17T00:00:00Z".into(),
        modified_at: None,
    };

    SseEvent::DocumentUpsert(IndexerEnvelope {
        uri: uri.into(),
        record: doc,
        indexed_at: "2026-04-17T00:00:00Z".into(),
        deleted_at: None,
    })
}

fn sse_doc_delete(uri: &str) -> SseEvent {
    use crate::indexer::sse::events::SseDeletePayload;
    SseEvent::DocumentDelete(SseDeletePayload { uri: uri.into() })
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
    install_ws(
        &mut keeper,
        &ws_a,
        DirectoryTree::from_records(std::iter::empty()),
        ContentKey([0u8; 32]),
        0,
    );
    install_ws(
        &mut keeper,
        &ws_b,
        DirectoryTree::from_records(std::iter::empty()),
        ContentKey([1u8; 32]),
        0,
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

/// A keyring member entry carrying `group_key` wrapped for the test
/// identity, anchored to the workspace's stable URI.
fn wrapped_member(group_key: &ContentKey, workspace_id: &str) -> crate::records::KeyringMember {
    use crate::crypto::{self, OsRng, WrapContext};
    let kp = test_keypair();
    let wrapped_key = crypto::wrap_key(
        group_key,
        &kp.public_keys(),
        TEST_DID,
        &WrapContext::Keyring { uri: workspace_id },
        &mut OsRng,
    )
    .unwrap();
    crate::records::KeyringMember {
        wrapped_key,
        role: crate::records::Role::Manager,
    }
}

/// Build a keyring record with the test identity as sole member holding
/// `current_key` at `rotation`, plus a `keyHistory` entry per `(rotation,
/// key)` in `history`. Genesis-shaped (head URI == workspace_id).
fn keyring_record(
    workspace_id: &str,
    rotation: u64,
    current_key: &ContentKey,
    history: &[(u64, &ContentKey)],
) -> crate::records::Keyring {
    use crate::atproto::AtBytes;
    use crate::records::{EncryptedMetadata, KeyHistoryEntry, Keyring, SCHEMA_VERSION};

    Keyring {
        opake_version: SCHEMA_VERSION,
        algo: "aes-256-gcm".into(),
        members: vec![wrapped_member(current_key, workspace_id)],
        rotation,
        key_history: history
            .iter()
            .map(|(rot, key)| KeyHistoryEntry {
                rotation: *rot,
                members: vec![wrapped_member(key, workspace_id)],
            })
            .collect(),
        encrypted_metadata: EncryptedMetadata {
            ciphertext: AtBytes {
                encoded: String::new(),
            },
            nonce: AtBytes {
                encoded: String::new(),
            },
        },
        supersedes: None,
        workspace_id: Some(workspace_id.into()),
        created_at: "2026-04-17T00:00:00Z".into(),
        modified_at: None,
    }
}

fn keyring_upsert_of(record: crate::records::Keyring, uri: &str) -> SseEvent {
    SseEvent::KeyringUpsert(IndexerEnvelope {
        uri: uri.into(),
        record,
        indexed_at: "2026-04-17T00:00:00Z".into(),
        deleted_at: None,
    })
}

/// A keyring-encrypted directory at a specific rotation, tagged with its
/// workspace id so keeper scope-routing lands it in the right tree.
fn keyring_dir(
    name: &str,
    keyring_uri: &str,
    group_key: &ContentKey,
    rotation: u64,
    entries: Vec<String>,
) -> crate::records::Directory {
    use crate::crypto::OsRng;
    let (key_wrapping, encrypted_metadata) =
        crate::directories::encrypt_keyring_directory_envelope(
            name,
            None,
            keyring_uri,
            group_key,
            rotation,
            &mut OsRng,
        )
        .unwrap();
    let entries = entries
        .into_iter()
        .map(|target| crate::records::ListingEntry::new(target, "bafytest"))
        .collect();
    let mut dir = crate::records::Directory {
        entries,
        ..crate::records::Directory::new(
            key_wrapping,
            encrypted_metadata,
            "2026-04-17T00:00:00Z".into(),
        )
    };
    dir.workspace_id = Some(keyring_uri.into());
    dir
}

fn dir_upsert_of(record: crate::records::Directory, uri: &str) -> SseEvent {
    SseEvent::DirectoryUpsert(IndexerEnvelope {
        uri: uri.into(),
        record,
        indexed_at: "2026-04-17T00:00:00Z".into(),
        deleted_at: None,
    })
}

// A live projection must adopt a rotation from the event alone: the new
// group key is unwrapped straight from the keyring record, the prior key is
// archived, and cached names re-decrypt — no reload. Regression for the
// keeper that used to bump the counter and blank the names to "?".
// spec:key-rotation § Live projections adopt a rotation completely
#[test]
fn rotation_event_keeps_names_readable_across_rotation() {
    const WS_URI: &str = "at://did:plc:test/app.opake.keyring/rot";
    const ROOT_URI2: &str = "at://did:plc:test/app.opake.directory/rot-root";
    const SUB_URI: &str = "at://did:plc:test/app.opake.directory/rot-sub";
    const NEW_URI: &str = "at://did:plc:test/app.opake.directory/rot-new";
    use crate::crypto::{self, OsRng};

    let kp = test_keypair();
    let key0 = crypto::generate_content_key(&mut OsRng);
    let key1 = crypto::generate_content_key(&mut OsRng);

    // Rotation-0 tree: root + a "Reports" subdir, both wrapped under key0.
    let tree = {
        let root = keyring_dir("/", WS_URI, &key0, 0, vec![SUB_URI.into()]);
        let sub = keyring_dir("Reports", WS_URI, &key0, 0, vec![]);
        let mut tree = DirectoryTree::from_records(vec![
            (ROOT_URI2.to_string(), root),
            (SUB_URI.to_string(), sub),
        ]);
        // Mirror a fresh bootstrap: names readable at rotation 0.
        let hist: Vec<crate::workspace::HistoricalKey> = Vec::new();
        let view = crate::workspace::GroupKeys {
            current_rotation: 0,
            current: &key0,
            historical: &hist,
        };
        let group_keys = HashMap::from([(WS_URI.to_string(), view)]);
        tree.decrypt_names_with_group_keys(TEST_DID, &kp.private_keys(), &group_keys);
        assert_eq!(tree.directory_name(SUB_URI), Some("Reports"));
        tree
    };

    let mut keeper = cabinet_keeper();
    keeper.install_workspace_tree(
        WS_URI.into(),
        tree,
        key0.clone(),
        0,
        Vec::new(),
        kp.x25519_private,
        kp.ml_kem_private,
    );

    let sink = RecordingSink::new();
    keeper.watch_workspace(WS_URI.into(), ROOT_URI2.into(), sink.callback());
    let before = sink.count();

    // Rotation event: rotation 1, key1 current, key0 pushed into history.
    let record = keyring_record(WS_URI, 1, &key1, &[(0, &key0)]);
    keeper
        .apply_event(&keyring_upsert_of(record, WS_URI))
        .unwrap();

    // Adoption notified watchers.
    assert_eq!(sink.count(), before + 1, "rotation adoption fires watchers");

    // The pre-rotation name resolves through the archived key — readable,
    // not blanked to "?" or "".
    assert_eq!(
        keeper
            .workspace_tree(WS_URI)
            .and_then(|t| t.directory_name(SUB_URI)),
        Some("Reports"),
        "pre-rotation names survive an in-place rotation"
    );

    // A directory written under rotation 1 decrypts via the adopted key,
    // with no re-bootstrap (post-rotation upload readable by a live peer).
    let new_dir = keyring_dir("Q3", WS_URI, &key1, 1, vec![]);
    keeper
        .apply_event(&dir_upsert_of(new_dir, NEW_URI))
        .unwrap();
    assert_eq!(
        keeper
            .workspace_tree(WS_URI)
            .and_then(|t| t.directory_name(NEW_URI)),
        Some("Q3"),
        "post-rotation entries decrypt via the adopted key without reload"
    );
}

#[test]
fn keyring_upsert_without_rotation_bump_is_noop() {
    const WS_URI: &str = "at://did:plc:test/app.opake.keyring/abc";
    use crate::crypto::{self, OsRng};

    let key = crypto::generate_content_key(&mut OsRng);
    let mut keeper = cabinet_keeper();
    install_ws(
        &mut keeper,
        WS_URI,
        DirectoryTree::from_records(std::iter::empty()),
        key.clone(),
        5,
    );

    let sink = RecordingSink::new();
    keeper.watch_workspace(
        WS_URI.into(),
        "at://did:plc:test/app.opake.directory/ws-abc".into(),
        sink.callback(),
    );
    let before = sink.count();

    // Same rotation → metadata-only supersede; nothing key-derived changes.
    let record = keyring_record(WS_URI, 5, &key, &[]);
    keeper
        .apply_event(&keyring_upsert_of(record, WS_URI))
        .unwrap();

    assert_eq!(sink.count(), before, "no watcher fire expected");
}

#[test]
fn held_tree_boxes_key_material_out_of_line() {
    // Both `HeldTree` variants stash their hybrid private keys behind a
    // `Box`, so Rust's `max(variant size)` layout never inlines the
    // ~2432-byte ML-KEM key into the enum. Without the boxes the enum
    // would balloon to cabinet-key size for every workspace tree too.
    use crate::crypto::{MlKemPrivateKey, X25519PrivateKey};
    use std::mem::size_of;

    // The raw key bytes still cost what they cost — they just live on the
    // heap inside `HybridPrivateKeys`.
    assert_eq!(size_of::<MlKemPrivateKey>(), 2400);
    assert_eq!(size_of::<X25519PrivateKey>(), 32);

    // Inline storage would push HeldTree to >= 2432 bytes; with the boxes,
    // the enum stays small. 2000 is a generous ceiling that still catches a
    // regression to inline storage in either variant.
    assert!(
        size_of::<super::HeldTree>() < 2000,
        "HeldTree is {} bytes — key material may have regressed to inline storage",
        size_of::<super::HeldTree>(),
    );
}
