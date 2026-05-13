use crate::client::RequestBody;
use crate::manager::types::{FileContext, MutationOutcome};
use crate::records::Directory;

#[test]
fn file_context_owner_did_cabinet() {
    use crate::cabinet::Cabinet;
    use crate::crypto::OsRng;
    use crate::storage::Identity;

    let id = Identity::generate("did:plc:alice", &mut OsRng);
    let cabinet = Cabinet::from_identity(&id).unwrap();
    let ctx = FileContext::Cabinet(cabinet);

    assert_eq!(ctx.owner_did(), "did:plc:alice");
    assert!(ctx.is_cabinet());
    assert!(!ctx.is_workspace());
}

#[test]
fn file_context_owner_did_workspace() {
    use crate::crypto::{generate_content_key, OsRng};
    use crate::workspace::Workspace;

    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:bob/app.opake.keyring/xyz".into(),
        "Bob's WS".into(),
        None,
        "did:plc:bob".into(),
        gk,
        1,
        Vec::new(),
    );
    let ctx = FileContext::Workspace(ws);

    assert_eq!(ctx.owner_did(), "did:plc:bob");
    assert!(ctx.is_workspace());
    assert!(!ctx.is_cabinet());
}

/// Cabinet delete must atomically (a) delete the document record AND
/// (b) remove the document URI from the parent directory's entries.
/// Regression: the old code had an escape hatch that skipped (b) when the
/// caller didn't pass a parent URI, leaving dangling entries in the PDS
/// directory record (and therefore in any consumer that mirrored it).
#[tokio::test]
async fn cabinet_delete_removes_doc_and_unlinks_parent_entry() {
    use crate::client::{Session, LegacySession, XrpcClient};
    use crate::crypto::OsRng;
    use crate::directories::tests::{
        dummy_directory_with_entries, get_record_response, put_record_response,
    };
    use crate::opake::Opake;
    use crate::storage::{Identity, NoopStorage};
    use crate::test_utils::MockTransport;

    const DID: &str = "did:plc:test";
    const PARENT_URI: &str = "at://did:plc:test/app.opake.directory/self";
    const DOC_URI: &str = "at://did:plc:test/app.opake.document/doc1";
    const OTHER_DOC_URI: &str = "at://did:plc:test/app.opake.document/keep";

    // Seed the mock: getRecord for the parent directory (loaded by
    // prepare_remove_entry), then applyWrites (the atomic delete + update).
    let mock = MockTransport::new();
    mock.enqueue(get_record_response(
        PARENT_URI,
        &dummy_directory_with_entries("root", vec![DOC_URI.into(), OTHER_DOC_URI.into()]),
    ));
    mock.enqueue(put_record_response(PARENT_URI));

    let session = Session::Legacy(LegacySession {
        did: DID.into(),
        handle: "test.handle".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    let client = XrpcClient::with_session(mock.clone(), "https://pds.test".into(), session);
    let identity = Identity::generate(DID, &mut OsRng);
    let mut opake = Opake::new(
        client,
        DID.into(),
        identity,
        OsRng,
        NoopStorage,
        || 1_700_000_000_000_000,
    )
    .unwrap();

    let ctx = opake.cabinet_context().unwrap();
    let mut mgr = opake.file_manager(&ctx);
    let outcome = mgr.delete(DOC_URI, PARENT_URI).await.unwrap();

    assert!(matches!(outcome, MutationOutcome::Applied));

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2, "expected getRecord + applyWrites");
    assert!(reqs[0].url.contains("getRecord"), "first call is fetch");
    assert!(reqs[1].url.contains("applyWrites"), "second call is atomic write");

    // Assert the applyWrites body carries BOTH ops in one batch, and the
    // directory update actually prunes the deleted doc (but keeps siblings).
    let Some(RequestBody::Json(body)) = &reqs[1].body else {
        panic!("applyWrites body should be JSON");
    };
    let writes = body["writes"].as_array().expect("writes array");
    assert_eq!(writes.len(), 2, "atomic batch: [delete doc, update dir]");

    let delete_op = &writes[0];
    assert_eq!(delete_op["$type"], "com.atproto.repo.applyWrites#delete");
    assert_eq!(delete_op["collection"], "app.opake.document");
    assert_eq!(delete_op["rkey"], "doc1");

    let update_op = &writes[1];
    assert_eq!(update_op["$type"], "com.atproto.repo.applyWrites#update");
    assert_eq!(update_op["collection"], "app.opake.directory");
    let updated: Directory = serde_json::from_value(update_op["value"].clone()).unwrap();
    assert_eq!(
        updated.entries.iter().map(|e| e.target.as_str()).collect::<Vec<_>>(),
        vec![OTHER_DOC_URI],
        "deleted doc URI must be pruned from parent.entries; siblings preserved",
    );
    assert_eq!(
        updated.modified_at.as_deref(),
        Some("2023-11-14T22:13:20.000000Z")
    );
}

/// Workspace delete where the caller owns the parent directory is the same
/// "atomic delete + parent update" shape as the cabinet path, not a proposal.
/// The owner branch writes directly to the canonical directory record; the
/// Applied / Proposed split is entirely driven by `dir_owner == self.opake.did`.
#[tokio::test]
async fn workspace_owner_delete_is_applied_not_proposed() {
    use crate::client::{LegacySession, Session, XrpcClient};
    use crate::crypto::{generate_content_key, OsRng};
    use crate::directories::tests::{
        dummy_directory_with_entries, get_record_response, put_record_response,
    };
    use crate::manager::types::FileContext;
    use crate::opake::Opake;
    use crate::storage::{Identity, NoopStorage};
    use crate::test_utils::MockTransport;
    use crate::workspace::Workspace;

    // The current user is also the parent-directory owner — so the write
    // goes through the canonical directory record, not a directoryUpdate.
    const DID: &str = "did:plc:alice";
    const PARENT_URI: &str = "at://did:plc:alice/app.opake.directory/self";
    const DOC_URI: &str = "at://did:plc:alice/app.opake.document/doc1";
    const OTHER_DOC_URI: &str = "at://did:plc:alice/app.opake.document/keep";
    const KEYRING_URI: &str = "at://did:plc:alice/app.opake.keyring/ws1";

    let mock = MockTransport::new();
    mock.enqueue(get_record_response(
        PARENT_URI,
        &dummy_directory_with_entries("root", vec![DOC_URI.into(), OTHER_DOC_URI.into()]),
    ));
    mock.enqueue(put_record_response(PARENT_URI));

    let session = Session::Legacy(LegacySession {
        did: DID.into(),
        handle: "alice.test".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    let client = XrpcClient::with_session(mock.clone(), "https://pds.test".into(), session);
    let identity = Identity::generate(DID, &mut OsRng);
    let mut opake = Opake::new(
        client,
        DID.into(),
        identity,
        OsRng,
        NoopStorage,
        || 1_700_000_000_000_000,
    )
    .unwrap();

    let group_key = generate_content_key(&mut OsRng);
    let workspace = Workspace::from_keyring(
        KEYRING_URI.into(),
        "Alice's workspace".into(),
        None,
        DID.into(),
        group_key,
        1,
        Vec::new(),
    );
    let ctx = FileContext::Workspace(workspace);
    let mut mgr = opake.file_manager(&ctx);
    let outcome = mgr.delete(DOC_URI, PARENT_URI).await.unwrap();

    assert!(outcome.is_applied(), "owner path must produce Applied");

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2, "expected getRecord + applyWrites (no proposal)");
    let Some(RequestBody::Json(body)) = &reqs[1].body else {
        panic!("applyWrites body should be JSON");
    };
    let writes = body["writes"].as_array().expect("writes array");
    assert_eq!(writes.len(), 2, "atomic batch: [delete doc, update dir]");
    assert_eq!(writes[0]["$type"], "com.atproto.repo.applyWrites#delete");
    assert_eq!(writes[1]["$type"], "com.atproto.repo.applyWrites#update");
    assert_eq!(writes[1]["collection"], "app.opake.directory");
}

/// Workspace delete where the caller does NOT own the parent directory
/// is rejected pending the federation rewrite (curatorial-supersede cascade
/// not yet wired through the manager).
#[tokio::test]
async fn workspace_non_owner_delete_rejected_pending_cascade() {
    use crate::client::{LegacySession, Session, XrpcClient};
    use crate::crypto::{generate_content_key, OsRng};
    use crate::error::Error;
    use crate::manager::types::FileContext;
    use crate::opake::Opake;
    use crate::storage::{Identity, NoopStorage};
    use crate::test_utils::MockTransport;
    use crate::workspace::Workspace;

    const ALICE_DID: &str = "did:plc:alice";
    const BOB_DID: &str = "did:plc:bob";
    const PARENT_URI: &str = "at://did:plc:bob/app.opake.directory/self";
    const DOC_URI: &str = "at://did:plc:alice/app.opake.document/doc1";
    const KEYRING_URI: &str = "at://did:plc:bob/app.opake.keyring/ws1";

    let mock = MockTransport::new();
    let session = Session::Legacy(LegacySession {
        did: ALICE_DID.into(),
        handle: "alice.test".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    let client = XrpcClient::with_session(mock.clone(), "https://pds.test".into(), session);
    let identity = Identity::generate(ALICE_DID, &mut OsRng);
    let mut opake = Opake::new(
        client,
        ALICE_DID.into(),
        identity,
        OsRng,
        NoopStorage,
        || 1_700_000_000_000_000,
    )
    .unwrap();

    let group_key = generate_content_key(&mut OsRng);
    let workspace = Workspace::from_keyring(
        KEYRING_URI.into(),
        "Bob's workspace".into(),
        None,
        BOB_DID.into(),
        group_key,
        1,
        Vec::new(),
    );
    let ctx = FileContext::Workspace(workspace);
    let mut mgr = opake.file_manager(&ctx);
    let err = mgr.delete(DOC_URI, PARENT_URI).await.unwrap_err();
    assert!(
        matches!(err, Error::Unimplemented(ref msg) if msg.contains("cascade")),
        "expected federation-cascade stub error, got {err:?}"
    );
}

#[test]
fn mutation_outcome_predicates() {
    let applied = MutationOutcome::Applied;
    assert!(applied.is_applied());
}
