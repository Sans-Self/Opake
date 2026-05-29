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
        vec!["did:plc:bob".into()],
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

// Federation-era workspace delete tests live in `workspace_delete_cascade`
// below — the owner vs non-owner asymmetry from the legacy model is gone:
// every workspace delete runs the same cascade.

#[test]
fn mutation_outcome_predicates() {
    let applied = MutationOutcome::Applied;
    assert!(applied.is_applied());
}

// ---------------------------------------------------------------------------
// Workspace upload — federation cascade path
//
// The federation rewrite eliminates the owner/non-owner branch: every
// workspace upload runs a curatorial supersede cascade. The doc lands on
// the caller's PDS; the cascade writes a new (or genesis) root directory
// on the caller's PDS too, with the new doc threaded into its listing.
// ---------------------------------------------------------------------------

mod workspace_upload_cascade {
    use crate::client::{HttpResponse, LegacySession, RequestBody, Session, XrpcClient};
    use crate::crypto::{generate_content_key, OsRng};
    use crate::directories::tests::dummy_directory_with_entries;
    use crate::manager::types::FileContext;
    use crate::opake::Opake;
    use crate::records::Directory;
    use crate::storage::{Identity, NoopStorage};
    use crate::test_utils::MockTransport;
    use crate::workspace::Workspace;

    const ALICE_DID: &str = "did:plc:alice";
    const KEYRING_URI: &str = "at://did:plc:alice/app.opake.keyring/ws1";
    const INDEXER_URL: &str = "https://indexer.test";

    /// Mock `/api/workspace/chain-head` response.
    fn chain_head_response(
        keyring_head: Option<(&str, &str)>,
        root_head: Option<(&str, &str)>,
    ) -> HttpResponse {
        let body = serde_json::json!({
            "workspace_id": KEYRING_URI,
            "keyring": keyring_head.map(|(uri, cid)| serde_json::json!({
                "head_uri": uri,
                "head_cid": cid,
            })),
            "root_directory": root_head.map(|(uri, cid)| serde_json::json!({
                "head_uri": uri,
                "head_cid": cid,
            })),
        });
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    /// Mock a DID document resolution response.
    fn did_doc_response(did: &str, pds_url: &str) -> HttpResponse {
        let body = serde_json::json!({
            "id": did,
            "alsoKnownAs": [],
            "service": [{
                "id": "#atproto_pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": pds_url,
            }]
        });
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    /// Mock a getRecord response wrapping a directory.
    fn get_directory_response(uri: &str, cid: &str, directory: &Directory) -> HttpResponse {
        let body = serde_json::json!({
            "uri": uri,
            "cid": cid,
            "value": directory,
        });
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    /// Mock the PDS `uploadBlob` response. `prepare_upload_keyring`
    /// streams the ciphertext blob to the PDS first, then we use the
    /// returned ref in the document record.
    fn upload_blob_response() -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "blob": {
                    "$type": "blob",
                    "ref": { "$link": "bafyblob" },
                    "mimeType": "application/octet-stream",
                    "size": 64,
                }
            }))
            .unwrap(),
        }
    }

    fn opake_for_alice(mock: MockTransport) -> Opake<MockTransport, OsRng, NoopStorage> {
        let session = Session::Legacy(LegacySession {
            did: ALICE_DID.into(),
            handle: "alice.test".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        let client = XrpcClient::with_session(mock, "https://pds.alice".into(), session);
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
        opake.set_indexer_url(INDEXER_URL.into());
        opake
    }

    fn workspace_for_alice() -> Workspace {
        let group_key = generate_content_key(&mut OsRng);
        Workspace::from_keyring(
            KEYRING_URI.into(),
            "Alice's WS".into(),
            None,
            ALICE_DID.into(),
            group_key,
            1,
            Vec::new(),
            vec![ALICE_DID.into()],
        )
    }

    /// Genesis path: no indexed root yet. Cascade leaf is `Genesis` at the
    /// stable `ws-{rkey}` rkey. New entries = just the freshly uploaded doc.
    #[tokio::test]
    async fn genesis_root_cascade_when_no_indexed_root() {
        let mock = MockTransport::new();

        // 1. chain-head endpoint: keyring exists (genesis), root_directory None
        mock.enqueue(chain_head_response(
            Some((KEYRING_URI, "bafygenesis")),
            None,
        ));
        // 2. uploadBlob for the encrypted ciphertext
        mock.enqueue(upload_blob_response());
        // 3. createRecord for the document — explicit cid for downstream
        // assertion that it gets threaded into the genesis root's listing.
        let doc_uri = format!("at://{ALICE_DID}/app.opake.document/doc1");
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": doc_uri,
                "cid": "bafydoc1",
            }))
            .unwrap(),
        });
        // 3. putRecord for the genesis root (stable rkey ws-ws1)
        let root_uri = format!("at://{ALICE_DID}/app.opake.directory/ws-ws1");
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": root_uri,
                "cid": "bafyrootgenesis",
            }))
            .unwrap(),
        });

        let mut opake = opake_for_alice(mock.clone());
        let workspace = workspace_for_alice();
        let ctx = FileContext::Workspace(workspace);
        let mut mgr = opake.file_manager(&ctx);

        let req = super::super::types::UploadRequest {
            plaintext: b"hello",
            filename: "report.pdf",
            mime_type: "application/pdf",
            description: None,
            tags: &[],
            directory_uri: None,
        };
        let result = mgr.upload(&req).await.unwrap();
        assert!(result.outcome.is_applied());

        let reqs = mock.requests();
        // Some calls in the doc encryption path may have happened too —
        // the structural assertions below check the *content* of the
        // chain-head + cascade writes rather than exact call count.
        let chain_head_idx = reqs
            .iter()
            .position(|r| r.url.contains("/api/workspace/chain-head"))
            .expect("chain-head endpoint must be called");
        assert!(reqs[chain_head_idx]
            .url
            .contains(&format!("workspace_id={KEYRING_URI}")));

        // Genesis is now TID-rkeyed via createRecord; the writer stamps
        // `isWorkspaceRoot: true` on the record so the indexer can claim
        // the chain head via compare-and-set.
        let create_root_idx = reqs
            .iter()
            .enumerate()
            .filter_map(|(idx, r)| {
                if !r.url.contains("createRecord") {
                    return None;
                }
                let body = match &r.body {
                    Some(RequestBody::Json(v)) => v,
                    _ => return None,
                };
                if body["collection"] == "app.opake.directory" {
                    Some(idx)
                } else {
                    None
                }
            })
            .next()
            .expect("genesis root must use createRecord with isWorkspaceRoot");

        match &reqs[create_root_idx].body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], "app.opake.directory");
                let written: Directory =
                    serde_json::from_value(v["record"].clone()).expect("record body");
                assert!(written.supersedes.is_none(), "genesis has no supersedes");
                assert!(written.is_workspace_root, "genesis must be flagged as root");
                assert_eq!(written.workspace_id.as_deref(), Some(KEYRING_URI));
                assert_eq!(written.entries.len(), 1);
                assert_eq!(written.entries[0].target, doc_uri);
                assert_eq!(written.entries[0].target_cid.cid, "bafydoc1");
            }
            _ => panic!("expected JSON body"),
        }
    }

    /// Supersede path: indexer reports an existing root. Cascade fetches
    /// the prior root record (across DIDs), copies its key wrapping +
    /// encrypted metadata forward, appends the new doc as a listing entry,
    /// and supersedes via `createRecord` (TID rkey on caller's PDS).
    #[tokio::test]
    async fn supersede_root_cascade_appends_new_entry() {
        let mock = MockTransport::new();

        // The prior root lives on Alice's PDS (same DID — common case)
        let prior_root_uri = format!("at://{ALICE_DID}/app.opake.directory/ws-ws1");
        let prior_doc_uri = format!("at://{ALICE_DID}/app.opake.document/preexisting");

        // 1. chain-head: root exists at prior_root_uri/cid
        mock.enqueue(chain_head_response(
            Some((KEYRING_URI, "bafygenesis")),
            Some((&prior_root_uri, "bafyrootcurrent")),
        ));

        // 2. uploadBlob for the doc's ciphertext
        mock.enqueue(upload_blob_response());
        // 3. createRecord for the doc — explicit body with the cid we
        // assert downstream gets threaded into the cascade's listing.
        let doc_uri = format!("at://{ALICE_DID}/app.opake.document/doc2");
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": doc_uri,
                "cid": "bafydoc2",
            }))
            .unwrap(),
        });

        // 3. fetch_chain_node walks DID→PDS then getRecord against the prior root
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.alice"));
        let prior_root = dummy_directory_with_entries("root", vec![prior_doc_uri.clone()]);
        mock.enqueue(get_directory_response(
            &prior_root_uri,
            "bafyrootcurrent",
            &prior_root,
        ));

        // 4. createRecord for the new root (cascade-supersede write — TID rkey)
        let new_root_uri = format!("at://{ALICE_DID}/app.opake.directory/3supernew");
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": new_root_uri,
                "cid": "bafyrootnew",
            }))
            .unwrap(),
        });

        let mut opake = opake_for_alice(mock.clone());
        let workspace = workspace_for_alice();
        let ctx = FileContext::Workspace(workspace);
        let mut mgr = opake.file_manager(&ctx);

        let req = super::super::types::UploadRequest {
            plaintext: b"hello",
            filename: "second.pdf",
            mime_type: "application/pdf",
            description: None,
            tags: &[],
            directory_uri: None,
        };
        let result = mgr.upload(&req).await.unwrap();
        assert!(result.outcome.is_applied());

        let reqs = mock.requests();
        // The cascade leaf write is the LAST createRecord that's not the doc.
        let cascade_write = reqs
            .iter()
            .filter(|r| r.url.contains("createRecord"))
            .last()
            .expect("must have at least one createRecord");

        match &cascade_write.body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], "app.opake.directory");
                let written: Directory =
                    serde_json::from_value(v["record"].clone()).expect("record body");
                assert_eq!(written.supersedes.as_deref(), Some(prior_root_uri.as_str()));
                assert_eq!(written.workspace_id.as_deref(), Some(KEYRING_URI));
                // Editor-additivity in action: prior entry retained,
                // new entry appended.
                assert_eq!(written.entries.len(), 2);
                assert_eq!(written.entries[0].target, prior_doc_uri);
                assert_eq!(written.entries[1].target, doc_uri);
                assert_eq!(written.entries[1].target_cid.cid, "bafydoc2");
            }
            _ => panic!("expected JSON body"),
        }
    }

    /// A non-member writing into a workspace they don't have access to
    /// still routes through the same code path — the indexer rejects the
    /// supersede at validation time, but the client doesn't try to short-
    /// circuit. This test pins the "no local pre-checks" contract by
    /// running a non-owner upload (Bob's PDS into Alice's workspace) and
    /// verifying the cascade write completes locally with `supersedes`
    /// pointing at Alice's root.
    #[tokio::test]
    async fn non_owner_supersede_cascade_still_writes_locally() {
        // Bob authors a supersede against Alice's workspace root. The
        // cascade write lands on Bob's PDS regardless of membership; the
        // indexer is the one that decides whether the supersede joins the
        // canonical chain.
        const BOB_DID: &str = "did:plc:bob";
        const ALICE_OWNED_KEYRING: &str = "at://did:plc:alice/app.opake.keyring/ws1";
        let prior_root_uri = "at://did:plc:alice/app.opake.directory/ws-ws1";

        let mock = MockTransport::new();
        mock.enqueue(chain_head_response(
            Some((ALICE_OWNED_KEYRING, "bafygenesis")),
            Some((prior_root_uri, "bafyrootcurrent")),
        ));
        mock.enqueue(upload_blob_response());
        let doc_uri = format!("at://{BOB_DID}/app.opake.document/bobdoc");
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": doc_uri,
                "cid": "bafybobdoc",
            }))
            .unwrap(),
        });
        // chain walk fetches prior root from Alice's PDS — DID doc resolve
        // + getRecord.
        mock.enqueue(did_doc_response("did:plc:alice", "https://pds.alice"));
        mock.enqueue(get_directory_response(
            prior_root_uri,
            "bafyrootcurrent",
            &dummy_directory_with_entries("root", vec![]),
        ));
        // Cascade write goes to BOB's PDS.
        let new_root_uri = format!("at://{BOB_DID}/app.opake.directory/3bobsupersede");
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": new_root_uri,
                "cid": "bafybobroot",
            }))
            .unwrap(),
        });

        let session = Session::Legacy(LegacySession {
            did: BOB_DID.into(),
            handle: "bob.test".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        let client = XrpcClient::with_session(mock.clone(), "https://pds.bob".into(), session);
        let identity = Identity::generate(BOB_DID, &mut OsRng);
        let mut opake = Opake::new(
            client,
            BOB_DID.into(),
            identity,
            OsRng,
            NoopStorage,
            || 1_700_000_000_000_000,
        )
        .unwrap();
        opake.set_indexer_url(INDEXER_URL.into());

        let workspace = Workspace::from_keyring(
            ALICE_OWNED_KEYRING.into(),
            "Alice's WS".into(),
            None,
            "did:plc:alice".into(),
            generate_content_key(&mut OsRng),
            1,
            Vec::new(),
            vec!["did:plc:alice".into()],
        );
        let ctx = FileContext::Workspace(workspace);
        let mut mgr = opake.file_manager(&ctx);

        let req = super::super::types::UploadRequest {
            plaintext: b"hi",
            filename: "n.pdf",
            mime_type: "application/pdf",
            description: None,
            tags: &[],
            directory_uri: None,
        };
        let result = mgr.upload(&req).await.unwrap();
        assert!(result.outcome.is_applied());

        let reqs = mock.requests();
        let cascade_write = reqs
            .iter()
            .filter(|r| r.url.contains("createRecord"))
            .last()
            .expect("cascade createRecord");
        match &cascade_write.body {
            Some(RequestBody::Json(v)) => {
                let written: Directory =
                    serde_json::from_value(v["record"].clone()).expect("record body");
                assert_eq!(written.supersedes.as_deref(), Some(prior_root_uri));
                assert_eq!(written.workspace_id.as_deref(), Some(ALICE_OWNED_KEYRING));
            }
            _ => panic!("expected JSON body"),
        }
    }

    // Deep cascade upload (subdirectory) is covered by unit tests on
    // `build_deep_cascade_levels` in `directories::cascade::tests`. The
    // full integration through `FileManager.upload` requires mocking the
    // indexer snapshot endpoint + multiple cross-PDS DID resolutions +
    // chained createRecord responses; the cost-to-coverage ratio favors
    // testing the cascade-builder primitive in isolation and trusting
    // the wire-up. We do exercise the root-only paths above as
    // integration tests because they're the high-traffic flow.
}

// ---------------------------------------------------------------------------
// Workspace delete — federation cascade path
//
// Delete is atomic: doc-delete + new-directory-record (superseding the
// indexed root head, entries pruned) batched in a single applyWrites.
// Owner/non-owner asymmetry is gone — both run the same cascade.
// ---------------------------------------------------------------------------

mod workspace_delete_cascade {
    use crate::client::{HttpResponse, LegacySession, RequestBody, Session, XrpcClient};
    use crate::crypto::{generate_content_key, OsRng};
    use crate::directories::tests::dummy_directory_with_entries;
    use crate::error::Error;
    use crate::manager::types::FileContext;
    use crate::opake::Opake;
    use crate::records::Directory;
    use crate::storage::{Identity, NoopStorage};
    use crate::test_utils::MockTransport;
    use crate::workspace::Workspace;

    const ALICE_DID: &str = "did:plc:alice";
    const KEYRING_URI: &str = "at://did:plc:alice/app.opake.keyring/ws1";
    const INDEXER_URL: &str = "https://indexer.test";

    fn chain_head_response(
        keyring_head: Option<(&str, &str)>,
        root_head: Option<(&str, &str)>,
    ) -> HttpResponse {
        let body = serde_json::json!({
            "workspace_id": KEYRING_URI,
            "keyring": keyring_head.map(|(uri, cid)| serde_json::json!({
                "head_uri": uri, "head_cid": cid,
            })),
            "root_directory": root_head.map(|(uri, cid)| serde_json::json!({
                "head_uri": uri, "head_cid": cid,
            })),
        });
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn did_doc_response(did: &str, pds_url: &str) -> HttpResponse {
        let body = serde_json::json!({
            "id": did,
            "alsoKnownAs": [],
            "service": [{
                "id": "#atproto_pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": pds_url,
            }]
        });
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn get_directory_response(uri: &str, cid: &str, directory: &Directory) -> HttpResponse {
        let body = serde_json::json!({
            "uri": uri,
            "cid": cid,
            "value": directory,
        });
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn opake_for_alice(mock: MockTransport) -> Opake<MockTransport, OsRng, NoopStorage> {
        let session = Session::Legacy(LegacySession {
            did: ALICE_DID.into(),
            handle: "alice.test".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        let client = XrpcClient::with_session(mock, "https://pds.alice".into(), session);
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
        opake.set_indexer_url(INDEXER_URL.into());
        opake
    }

    fn workspace_for_alice() -> Workspace {
        Workspace::from_keyring(
            KEYRING_URI.into(),
            "Alice's WS".into(),
            None,
            ALICE_DID.into(),
            generate_content_key(&mut OsRng),
            1,
            Vec::new(),
            vec![ALICE_DID.into()],
        )
    }

    /// Happy path: doc gets deleted, new root supersede gets written, both
    /// in one applyWrites batch.
    #[tokio::test]
    async fn supersedes_root_atomically_with_doc_delete() {
        let prior_root_uri = format!("at://{ALICE_DID}/app.opake.directory/ws-ws1");
        let doc_uri = format!("at://{ALICE_DID}/app.opake.document/doc1");
        let keep_uri = format!("at://{ALICE_DID}/app.opake.document/keep");

        let mock = MockTransport::new();
        mock.enqueue(chain_head_response(
            Some((KEYRING_URI, "bafygenesis")),
            Some((&prior_root_uri, "bafyrootcurrent")),
        ));
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.alice"));
        let prior = dummy_directory_with_entries("root", vec![doc_uri.clone(), keep_uri.clone()]);
        mock.enqueue(get_directory_response(
            &prior_root_uri,
            "bafyrootcurrent",
            &prior,
        ));
        // applyWrites returns just an OK; the test inspects the request body.
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: br#"{"results":[]}"#.to_vec(),
        });

        let mut opake = opake_for_alice(mock.clone());
        let ctx = FileContext::Workspace(workspace_for_alice());
        let mut mgr = opake.file_manager(&ctx);
        let outcome = mgr.delete(&doc_uri, &prior_root_uri).await.unwrap();
        assert!(outcome.is_applied());

        let reqs = mock.requests();
        let apply_writes = reqs
            .iter()
            .find(|r| r.url.contains("applyWrites"))
            .expect("must call applyWrites");
        match &apply_writes.body {
            Some(RequestBody::Json(v)) => {
                let writes = v["writes"].as_array().expect("writes array");
                assert_eq!(writes.len(), 2, "atomic [delete doc, create new root]");
                assert_eq!(writes[0]["$type"], "com.atproto.repo.applyWrites#delete");
                assert_eq!(writes[0]["collection"], "app.opake.document");
                assert_eq!(writes[1]["$type"], "com.atproto.repo.applyWrites#create");
                assert_eq!(writes[1]["collection"], "app.opake.directory");

                let new_root: Directory =
                    serde_json::from_value(writes[1]["value"].clone()).unwrap();
                assert_eq!(new_root.supersedes.as_deref(), Some(prior_root_uri.as_str()));
                assert_eq!(new_root.workspace_id.as_deref(), Some(KEYRING_URI));
                // Deleted doc pruned, sibling preserved.
                let targets: Vec<&str> =
                    new_root.entries.iter().map(|e| e.target.as_str()).collect();
                assert_eq!(targets, vec![keep_uri.as_str()]);
            }
            _ => panic!("expected JSON body"),
        }
    }

    // Subdirectory deletion (deep cascade) is covered by unit tests on
    // `build_deep_cascade_levels` in `directories::cascade::tests`. The
    // FileManager-level wire-up calls the cascade builder + an inline
    // ancestor walker; both are exercised independently. Wiring up a
    // full integration test would require mocking the indexer snapshot,
    // multiple PDS DID-doc resolutions, applyWrites return-CID parsing,
    // and chained createRecord responses — high cost, low marginal
    // coverage. We hand-test this path on the test accounts.

    /// Deleting from an empty/un-indexed workspace produces a clear NotFound
    /// — the cascade can't supersede a head that doesn't exist.
    #[tokio::test]
    async fn delete_without_indexed_root_errors_not_found() {
        let doc_uri = format!("at://{ALICE_DID}/app.opake.document/orphan");
        let parent_uri = format!("at://{ALICE_DID}/app.opake.directory/ws-ws1");

        let mock = MockTransport::new();
        mock.enqueue(chain_head_response(
            Some((KEYRING_URI, "bafygenesis")),
            None, // no root indexed
        ));

        let mut opake = opake_for_alice(mock);
        let ctx = FileContext::Workspace(workspace_for_alice());
        let mut mgr = opake.file_manager(&ctx);
        let err = mgr.delete(&doc_uri, &parent_uri).await.unwrap_err();
        assert!(
            matches!(err, Error::NotFound(_)),
            "expected NotFound, got {err:?}"
        );
    }

    /// Trying to delete a doc that's not in the root listing should fail
    /// loudly rather than silently producing an entries-unchanged cascade
    /// write.
    #[tokio::test]
    async fn delete_unknown_doc_errors_not_found() {
        let prior_root_uri = format!("at://{ALICE_DID}/app.opake.directory/ws-ws1");
        let other_doc_uri = format!("at://{ALICE_DID}/app.opake.document/other");
        let unknown_doc_uri = format!("at://{ALICE_DID}/app.opake.document/ghost");

        let mock = MockTransport::new();
        mock.enqueue(chain_head_response(
            Some((KEYRING_URI, "bafygenesis")),
            Some((&prior_root_uri, "bafyrootcurrent")),
        ));
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.alice"));
        mock.enqueue(get_directory_response(
            &prior_root_uri,
            "bafyrootcurrent",
            &dummy_directory_with_entries("root", vec![other_doc_uri]),
        ));

        let mut opake = opake_for_alice(mock);
        let ctx = FileContext::Workspace(workspace_for_alice());
        let mut mgr = opake.file_manager(&ctx);
        let err = mgr
            .delete(&unknown_doc_uri, &prior_root_uri)
            .await
            .unwrap_err();
        assert!(
            matches!(err, Error::NotFound(_)),
            "expected NotFound, got {err:?}"
        );
    }
}
