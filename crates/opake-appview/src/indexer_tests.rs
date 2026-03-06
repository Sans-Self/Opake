use std::sync::Arc;

use super::*;
use crate::db::Database;
use crate::firehose::events::IndexableEvent;
use crate::state::AppState;

fn test_state() -> Arc<AppState> {
    let db = Database::open_in_memory().unwrap();
    Arc::new(AppState::new(db))
}

#[test]
fn indexes_grant_create() {
    let state = test_state();
    let event = IndexableEvent::UpsertGrant {
        uri: "at://did:plc:owner/app.opake.grant/3abc".into(),
        owner_did: "did:plc:owner".into(),
        recipient_did: "did:plc:recipient".into(),
        document_uri: "at://did:plc:owner/app.opake.document/3xyz".into(),
        created_at: "2026-03-01T12:00:00Z".into(),
    };
    process_event(&state, &event, 1709330400000000).unwrap();

    let inbox = state
        .db
        .with_conn(|c| grants::list_inbox(c, "did:plc:recipient", 50, None))
        .unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].owner_did, "did:plc:owner");
}

#[test]
fn indexes_grant_delete() {
    let state = test_state();
    let uri = "at://did:plc:owner/app.opake.grant/3abc";

    let create = IndexableEvent::UpsertGrant {
        uri: uri.into(),
        owner_did: "did:plc:owner".into(),
        recipient_did: "did:plc:recipient".into(),
        document_uri: "at://did:plc:owner/app.opake.document/3xyz".into(),
        created_at: "2026-03-01T12:00:00Z".into(),
    };
    process_event(&state, &create, 1709330400000000).unwrap();

    let delete = IndexableEvent::DeleteGrant { uri: uri.into() };
    process_event(&state, &delete, 1709330500000000).unwrap();

    let inbox = state
        .db
        .with_conn(|c| grants::list_inbox(c, "did:plc:recipient", 50, None))
        .unwrap();
    assert!(inbox.is_empty());
}

#[test]
fn indexes_keyring_create() {
    let state = test_state();
    let event = IndexableEvent::UpsertKeyring {
        uri: "at://did:plc:owner/app.opake.keyring/3def".into(),
        owner_did: "did:plc:owner".into(),
        member_dids: vec!["did:plc:alice".into(), "did:plc:bob".into()],
    };
    process_event(&state, &event, 1709330400000000).unwrap();

    let alice = state
        .db
        .with_conn(|c| keyrings::list_keyrings_for_member(c, "did:plc:alice", 50, None))
        .unwrap();
    assert_eq!(alice.len(), 1);

    let bob = state
        .db
        .with_conn(|c| keyrings::list_keyrings_for_member(c, "did:plc:bob", 50, None))
        .unwrap();
    assert_eq!(bob.len(), 1);
}

#[test]
fn indexes_keyring_update_replaces_members() {
    let state = test_state();
    let uri = "at://did:plc:owner/app.opake.keyring/3def";

    let create = IndexableEvent::UpsertKeyring {
        uri: uri.into(),
        owner_did: "did:plc:owner".into(),
        member_dids: vec!["did:plc:alice".into(), "did:plc:bob".into()],
    };
    process_event(&state, &create, 1709330400000000).unwrap();

    // Bob removed, charlie added
    let update = IndexableEvent::UpsertKeyring {
        uri: uri.into(),
        owner_did: "did:plc:owner".into(),
        member_dids: vec!["did:plc:alice".into(), "did:plc:charlie".into()],
    };
    process_event(&state, &update, 1709330500000000).unwrap();

    let bob = state
        .db
        .with_conn(|c| keyrings::list_keyrings_for_member(c, "did:plc:bob", 50, None))
        .unwrap();
    assert!(bob.is_empty());

    let charlie = state
        .db
        .with_conn(|c| keyrings::list_keyrings_for_member(c, "did:plc:charlie", 50, None))
        .unwrap();
    assert_eq!(charlie.len(), 1);
}

#[test]
fn indexes_keyring_delete() {
    let state = test_state();
    let uri = "at://did:plc:owner/app.opake.keyring/3def";

    let create = IndexableEvent::UpsertKeyring {
        uri: uri.into(),
        owner_did: "did:plc:owner".into(),
        member_dids: vec!["did:plc:alice".into()],
    };
    process_event(&state, &create, 1709330400000000).unwrap();

    let delete = IndexableEvent::DeleteKeyring { uri: uri.into() };
    process_event(&state, &delete, 1709330500000000).unwrap();

    let alice = state
        .db
        .with_conn(|c| keyrings::list_keyrings_for_member(c, "did:plc:alice", 50, None))
        .unwrap();
    assert!(alice.is_empty());
}
