use super::*;
use crate::crypto::{generate_content_key, OsRng};
use crate::storage::{Identity, NoopStorage};
use crate::test_utils::MockTransport;

fn test_now_micros() -> u64 {
    1_700_000_000_000_000
}

fn make_test_opake() -> Opake<MockTransport, OsRng, NoopStorage> {
    let transport = MockTransport::new();
    let client = crate::client::XrpcClient::new(transport, "https://pds.example.com".into());
    let identity = Identity::generate("did:plc:test", &mut OsRng);
    Opake::new(
        client,
        "did:plc:test".into(),
        identity,
        OsRng,
        NoopStorage,
        test_now_micros,
    )
    .unwrap()
}

#[test]
fn did_returns_identity_did() {
    let opake = make_test_opake();
    assert_eq!(opake.did(), "did:plc:test");
}

#[test]
fn now_derives_rfc3339_from_injected_micros() {
    let opake = make_test_opake();
    // `test_now_micros()` returns 1_700_000_000_000_000 µs.
    assert_eq!(opake.now(), "2023-11-14T22:13:20.000000Z");
}

#[test]
fn cabinet_context_produces_cabinet() {
    let opake = make_test_opake();
    let context = opake.cabinet_context().unwrap();
    assert!(context.is_cabinet());
}

#[test]
fn cabinet_file_manager_is_owner() {
    let mut opake = make_test_opake();
    let context = opake.cabinet_context().unwrap();
    let mgr = opake.file_manager(&context);
    assert!(mgr.is_owner());
}

#[test]
fn workspace_file_manager_non_owner() {
    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:owner/app.opake.keyring/abc".into(),
        "Test WS".into(),
        None,
        "did:plc:owner".into(),
        gk,
        1,
        Vec::new(),
        vec!["did:plc:owner".into()],
    );

    let mut opake = make_test_opake();
    let context = FileContext::Workspace(ws);
    let mgr = opake.file_manager(&context);

    assert!(mgr.context().is_workspace());
    // Caller is "did:plc:test", owner is "did:plc:owner" → not owner.
    assert!(!mgr.is_owner());
}

#[test]
fn workspace_owner_is_detected() {
    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:test/app.opake.keyring/abc".into(),
        "My WS".into(),
        None,
        "did:plc:test".into(), // Same as the identity DID
        gk,
        1,
        Vec::new(),
        vec!["did:plc:test".into()],
    );

    let mut opake = make_test_opake();
    let context = FileContext::Workspace(ws);
    let mgr = opake.file_manager(&context);

    assert!(mgr.is_owner());
}

#[test]
fn workspace_admin_preserves_workspace() {
    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:owner/app.opake.keyring/abc".into(),
        "Admin WS".into(),
        None,
        "did:plc:owner".into(),
        gk,
        1,
        Vec::new(),
        vec!["did:plc:owner".into()],
    );

    let mut opake = make_test_opake();
    let admin = opake.workspace_admin(&ws);

    assert_eq!(admin.workspace().name, "Admin WS");
    assert_eq!(
        admin.workspace().keyring_uri(),
        "at://did:plc:owner/app.opake.keyring/abc"
    );
}

// ---------------------------------------------------------------------------
// Federation keyring supersede paths
// ---------------------------------------------------------------------------

mod keyring_supersede {
    use super::*;
    use crate::client::{HttpResponse, LegacySession, RequestBody, Session, XrpcClient};
    use crate::records::{AtBytes, Keyring, KeyringMember, Role, WrappedKey, SCHEMA_VERSION};
    use crate::test_utils::dummy_encrypted_metadata;

    const ALICE_DID: &str = "did:plc:alice";
    const BOB_DID: &str = "did:plc:bob";
    const WORKSPACE_ID: &str = "at://did:plc:alice/app.opake.keyring/genesis";
    const INDEXER_URL: &str = "https://indexer.test";

    fn chain_head_response(head_uri: &str, head_cid: &str) -> HttpResponse {
        let body = serde_json::json!({
            "workspace_id": WORKSPACE_ID,
            "keyring": { "head_uri": head_uri, "head_cid": head_cid },
            "root_directory": null,
        });
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn did_doc_response(did: &str, pds_url: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "id": did,
                "alsoKnownAs": [],
                "service": [{
                    "id": "#atproto_pds",
                    "type": "AtprotoPersonalDataServer",
                    "serviceEndpoint": pds_url,
                }]
            }))
            .unwrap(),
        }
    }

    fn get_keyring_response(uri: &str, cid: &str, keyring: &Keyring) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": uri,
                "cid": cid,
                "value": keyring,
            }))
            .unwrap(),
        }
    }

    fn create_record_response(uri: &str, cid: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": uri,
                "cid": cid,
            }))
            .unwrap(),
        }
    }

    fn keyring_with_members(members: Vec<(&str, Role)>) -> Keyring {
        Keyring {
            opake_version: SCHEMA_VERSION,
            algo: "aes-256-gcm".into(),
            members: members
                .into_iter()
                .map(|(did, role)| KeyringMember {
                    wrapped_key: WrappedKey {
                        did: did.into(),
                        ciphertext: AtBytes {
                            encoded: "AAAA".into(),
                        },
                        algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
                    },
                    role,
                })
                .collect(),
            rotation: 0,
            key_history: Vec::new(),
            encrypted_metadata: dummy_encrypted_metadata(),
            supersedes: None,
            workspace_id: None,
            created_at: "2026-03-01T00:00:00Z".into(),
            modified_at: None,
        }
    }

    /// Stamp a keyring as a supersede of `WORKSPACE_ID`'s genesis — the
    /// shape every steady-state workspace head record carries. Tests
    /// that exercise a keyring-supersede write path (every test in this
    /// module) build the prior head through this helper so the chain
    /// walk in `fetch_keyring_chain_head` can verify the genesis.
    fn as_supersede(mut k: Keyring) -> Keyring {
        k.supersedes = Some(WORKSPACE_ID.to_string());
        k.workspace_id = Some(WORKSPACE_ID.to_string());
        k
    }

    /// Genesis keyring at `WORKSPACE_ID`. Members can be anything; the
    /// chain walk only verifies the URI matches and `supersedes` is
    /// `None` (which `keyring_with_members` already produces).
    fn genesis_keyring(members: Vec<(&str, Role)>) -> Keyring {
        keyring_with_members(members)
    }

    fn opake_for(did: &str, mock: MockTransport) -> Opake<MockTransport, OsRng, NoopStorage> {
        let session = Session::Legacy(LegacySession {
            did: did.into(),
            handle: "test.handle".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        let client = XrpcClient::with_session(mock, format!("https://pds.{did}"), session);
        let identity = Identity::generate(did, &mut OsRng);
        let mut opake = Opake::new(
            client,
            did.into(),
            identity,
            OsRng,
            NoopStorage,
            test_now_micros,
        )
        .unwrap();
        opake.set_indexer_url(INDEXER_URL.into());
        opake
    }

    /// Happy path for `update_member_role`: manager promotes an existing
    /// editor. The supersede write carries the prior members list with
    /// just the targeted member's role bumped.
    // spec:workspace-membership § Role changes are manager-authored supersedes
    #[tokio::test]
    async fn update_member_role_writes_supersede_with_updated_role() {
        let prior_head_uri = format!("at://{ALICE_DID}/app.opake.keyring/3abc");

        let prior = as_supersede(keyring_with_members(vec![
            (ALICE_DID, Role::Manager),
            (BOB_DID, Role::Editor),
        ]));
        let genesis = genesis_keyring(vec![(ALICE_DID, Role::Manager)]);

        let mock = MockTransport::new();
        // 1. chain-head endpoint
        mock.enqueue(chain_head_response(&prior_head_uri, "bafyhead"));
        // 2. DID doc resolve for the head's authority (alice). Cached
        //    for the subsequent genesis fetch since both are on alice.
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.did-plc-alice"));
        // 3. getRecord for the prior keyring (the head)
        mock.enqueue(get_keyring_response(&prior_head_uri, "bafyhead", &prior));
        // 4. getRecord for the genesis — the chain walk follows `supersedes`
        //    back from the head and verifies the genesis URI matches
        //    WORKSPACE_ID before any authority logic runs.
        mock.enqueue(get_keyring_response(WORKSPACE_ID, "bafygenesis", &genesis));
        // 5. createRecord for the supersede write on alice's PDS
        let new_uri = format!("at://{ALICE_DID}/app.opake.keyring/3newsupersede");
        mock.enqueue(create_record_response(&new_uri, "bafynew"));

        let mut opake = opake_for(ALICE_DID, mock.clone());
        let outcome = opake
            .update_member_role(
                &WorkspaceId::from_resolved(WORKSPACE_ID),
                BOB_DID,
                Role::Manager,
            )
            .await
            .unwrap();
        assert!(outcome.is_applied());

        let reqs = mock.requests();
        let create = reqs
            .iter()
            .find(|r| r.url.contains("createRecord"))
            .expect("createRecord");
        match &create.body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], "app.opake.keyring");
                let written: Keyring =
                    serde_json::from_value(v["record"].clone()).expect("record body");
                assert_eq!(written.supersedes.as_deref(), Some(prior_head_uri.as_str()));
                assert_eq!(written.workspace_id.as_deref(), Some(WORKSPACE_ID));
                assert_eq!(written.members.len(), 2);
                // Bob promoted from Editor to Manager.
                let bob = written.members.iter().find(|m| m.did() == BOB_DID).unwrap();
                assert!(matches!(bob.role, Role::Manager));
                // Alice's role unchanged.
                let alice = written
                    .members
                    .iter()
                    .find(|m| m.did() == ALICE_DID)
                    .unwrap();
                assert!(matches!(alice.role, Role::Manager));
            }
            _ => panic!("expected JSON body"),
        }
    }

    /// Non-manager attempting any keyring supersede gets a clear local
    /// rejection before the write attempt. The indexer would also reject,
    /// but failing fast saves a roundtrip + provides a usable error.
    // spec:workspace-membership § Role changes are manager-authored supersedes
    #[tokio::test]
    async fn update_member_role_rejects_non_manager_caller() {
        let prior_head_uri = format!("at://{ALICE_DID}/app.opake.keyring/3abc");

        let prior = as_supersede(keyring_with_members(vec![
            (ALICE_DID, Role::Manager),
            (BOB_DID, Role::Editor),
        ]));
        let genesis = genesis_keyring(vec![(ALICE_DID, Role::Manager)]);

        let mock = MockTransport::new();
        mock.enqueue(chain_head_response(&prior_head_uri, "bafyhead"));
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.did-plc-alice"));
        mock.enqueue(get_keyring_response(&prior_head_uri, "bafyhead", &prior));
        mock.enqueue(get_keyring_response(WORKSPACE_ID, "bafygenesis", &genesis));

        // Bob (editor, not manager) attempts to promote himself.
        let mut opake = opake_for(BOB_DID, mock);
        let err = opake
            .update_member_role(
                &WorkspaceId::from_resolved(WORKSPACE_ID),
                BOB_DID,
                Role::Manager,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::error::Error::Auth(ref msg) if msg.contains("not a manager")),
            "expected Auth rejection, got {err:?}"
        );
    }

    /// `update_member_role` on a non-existent member surfaces a clear error
    /// rather than silently producing a no-op supersede.
    #[tokio::test]
    async fn update_member_role_errors_when_member_absent() {
        let prior_head_uri = format!("at://{ALICE_DID}/app.opake.keyring/3abc");

        let prior = as_supersede(keyring_with_members(vec![(ALICE_DID, Role::Manager)]));
        let genesis = genesis_keyring(vec![(ALICE_DID, Role::Manager)]);

        let mock = MockTransport::new();
        mock.enqueue(chain_head_response(&prior_head_uri, "bafyhead"));
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.did-plc-alice"));
        mock.enqueue(get_keyring_response(&prior_head_uri, "bafyhead", &prior));
        mock.enqueue(get_keyring_response(WORKSPACE_ID, "bafygenesis", &genesis));

        let mut opake = opake_for(ALICE_DID, mock);
        let err = opake
            .update_member_role(
                &WorkspaceId::from_resolved(WORKSPACE_ID),
                "did:plc:ghost",
                Role::Editor,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::error::Error::InvalidRecord(_)),
            "expected InvalidRecord, got {err:?}"
        );
    }

    /// Regression: `sync_workspace_by_uri` used to take a bare `&str` and
    /// compare it directly against each envelope's derived genesis. A
    /// caller holding a head URI post-supersede (what the WASM binding
    /// gets from JS) could pass it straight through and silently get
    /// `None` back — the workspace was never "not found," just queried
    /// under the wrong URI kind. Typing the parameter as `WorkspaceId`
    /// forces resolution first; this proves the lookup still matches an
    /// envelope whose own indexed URI (the head) differs from the
    /// genesis id being queried.
    ///
    /// See workspace-identity spec, "sync-by-URI accepts what its caller
    /// holds" (audit finding 4).
    // spec:workspace-identity § Head URI use is limited to head-record operations and resolution input
    #[tokio::test]
    #[allow(non_snake_case)] // bug__ regression-naming convention
    async fn bug__sync_workspace_by_uri_matches_envelope_by_derived_genesis() {
        let head_uri = format!("at://{ALICE_DID}/app.opake.keyring/3head");
        let prior = as_supersede(keyring_with_members(vec![(ALICE_DID, Role::Manager)]));

        let workspaces_response = serde_json::json!({
            "workspaces": [{
                "uri": head_uri,
                "record": prior,
                "indexedAt": "2026-03-01T00:00:00Z",
            }]
        });
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&workspaces_response).unwrap(),
        });

        // Bob isn't a member of `prior` — the group-key unwrap fails, but
        // that's a *found-but-undecryptable* result (`Some` with an
        // `error` field), never `None`. `None` only happens when no
        // envelope's derived genesis matches the query, which is exactly
        // the bug this guards against.
        let mut opake = opake_for(BOB_DID, mock);
        let result = opake
            .sync_workspace_by_uri(&WorkspaceId::from_resolved(WORKSPACE_ID))
            .await
            .unwrap();

        let result = result.expect("head-keyed envelope must match a genesis-keyed query");
        assert_eq!(result.keyring_uri, head_uri);
    }

    const CAROL_DID: &str = "did:plc:carol";

    /// Happy path for `leave_workspace`: an editor authors a self-removal
    /// supersede. No rotation — the leaver would have to mint the new
    /// group key, which buys nothing — so the remaining members' wraps,
    /// the rotation counter, and the key history all carry verbatim.
    // spec:workspace-membership § Removal rotates the group key; leave does not
    #[tokio::test]
    async fn leave_workspace_writes_self_removal_supersede() {
        let prior_head_uri = format!("at://{ALICE_DID}/app.opake.keyring/3abc");

        let prior = as_supersede(keyring_with_members(vec![
            (ALICE_DID, Role::Manager),
            (BOB_DID, Role::Editor),
            (CAROL_DID, Role::Viewer),
        ]));
        let genesis = genesis_keyring(vec![(ALICE_DID, Role::Manager)]);

        let mock = MockTransport::new();
        mock.enqueue(chain_head_response(&prior_head_uri, "bafyhead"));
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.did-plc-alice"));
        mock.enqueue(get_keyring_response(&prior_head_uri, "bafyhead", &prior));
        mock.enqueue(get_keyring_response(WORKSPACE_ID, "bafygenesis", &genesis));
        let new_uri = format!("at://{BOB_DID}/app.opake.keyring/3leave");
        mock.enqueue(create_record_response(&new_uri, "bafyleave"));

        let mut opake = opake_for(BOB_DID, mock.clone());
        let outcome = opake
            .leave_workspace(&WorkspaceId::from_resolved(WORKSPACE_ID))
            .await
            .unwrap();
        assert!(outcome.is_applied());

        let reqs = mock.requests();
        let create = reqs
            .iter()
            .find(|r| r.url.contains("createRecord"))
            .expect("createRecord");
        match &create.body {
            Some(RequestBody::Json(v)) => {
                assert_eq!(v["collection"], "app.opake.keyring");
                let written: Keyring =
                    serde_json::from_value(v["record"].clone()).expect("record body");
                assert_eq!(written.supersedes.as_deref(), Some(prior_head_uri.as_str()));
                assert_eq!(written.workspace_id.as_deref(), Some(WORKSPACE_ID));
                // Bob gone, alice + carol carried verbatim with roles intact.
                assert_eq!(written.members.len(), 2);
                assert!(written.members.iter().all(|m| m.did() != BOB_DID));
                let alice = written
                    .members
                    .iter()
                    .find(|m| m.did() == ALICE_DID)
                    .unwrap();
                assert!(matches!(alice.role, Role::Manager));
                let carol = written
                    .members
                    .iter()
                    .find(|m| m.did() == CAROL_DID)
                    .unwrap();
                assert!(matches!(carol.role, Role::Viewer));
                // No rotation on leave.
                assert_eq!(written.rotation, prior.rotation);
                assert_eq!(written.key_history.len(), prior.key_history.len());
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[tokio::test]
    async fn leave_workspace_rejects_non_member() {
        let prior_head_uri = format!("at://{ALICE_DID}/app.opake.keyring/3abc");

        let prior = as_supersede(keyring_with_members(vec![
            (ALICE_DID, Role::Manager),
            (BOB_DID, Role::Editor),
        ]));
        let genesis = genesis_keyring(vec![(ALICE_DID, Role::Manager)]);

        let mock = MockTransport::new();
        mock.enqueue(chain_head_response(&prior_head_uri, "bafyhead"));
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.did-plc-alice"));
        mock.enqueue(get_keyring_response(&prior_head_uri, "bafyhead", &prior));
        mock.enqueue(get_keyring_response(WORKSPACE_ID, "bafygenesis", &genesis));

        let mut opake = opake_for(CAROL_DID, mock);
        let err = opake
            .leave_workspace(&WorkspaceId::from_resolved(WORKSPACE_ID))
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("not a member"),
            "expected not-a-member error, got: {err}"
        );
    }

    // spec:workspace-membership § Leave guards — no orphaned workspaces
    #[tokio::test]
    async fn leave_workspace_rejects_last_member() {
        let prior_head_uri = format!("at://{ALICE_DID}/app.opake.keyring/3abc");

        let prior = as_supersede(keyring_with_members(vec![(ALICE_DID, Role::Manager)]));
        let genesis = genesis_keyring(vec![(ALICE_DID, Role::Manager)]);

        let mock = MockTransport::new();
        mock.enqueue(chain_head_response(&prior_head_uri, "bafyhead"));
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.did-plc-alice"));
        mock.enqueue(get_keyring_response(&prior_head_uri, "bafyhead", &prior));
        mock.enqueue(get_keyring_response(WORKSPACE_ID, "bafygenesis", &genesis));

        let mut opake = opake_for(ALICE_DID, mock);
        let err = opake
            .leave_workspace(&WorkspaceId::from_resolved(WORKSPACE_ID))
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("last member"),
            "expected last-member error, got: {err}"
        );
    }

    // spec:workspace-membership § Leave guards — no orphaned workspaces
    #[tokio::test]
    async fn leave_workspace_rejects_only_manager() {
        let prior_head_uri = format!("at://{ALICE_DID}/app.opake.keyring/3abc");

        let prior = as_supersede(keyring_with_members(vec![
            (ALICE_DID, Role::Manager),
            (BOB_DID, Role::Editor),
        ]));
        let genesis = genesis_keyring(vec![(ALICE_DID, Role::Manager)]);

        let mock = MockTransport::new();
        mock.enqueue(chain_head_response(&prior_head_uri, "bafyhead"));
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.did-plc-alice"));
        mock.enqueue(get_keyring_response(&prior_head_uri, "bafyhead", &prior));
        mock.enqueue(get_keyring_response(WORKSPACE_ID, "bafygenesis", &genesis));

        let mut opake = opake_for(ALICE_DID, mock);
        let err = opake
            .leave_workspace(&WorkspaceId::from_resolved(WORKSPACE_ID))
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("only manager"),
            "expected only-manager error, got: {err}"
        );
    }
}
