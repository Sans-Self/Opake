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
        "at://did:plc:owner/at.opake.keyring/abc".into(),
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
        "at://did:plc:test/at.opake.keyring/abc".into(),
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
        "at://did:plc:owner/at.opake.keyring/abc".into(),
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
        "at://did:plc:owner/at.opake.keyring/abc"
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
    const WORKSPACE_ID: &str = "at://did:plc:alice/at.opake.keyring/genesis";
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
            supersedes_cid: None,
            lineage: None,
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
        k.lineage = Some(WORKSPACE_ID.to_string());
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
        let prior_head_uri = format!("at://{ALICE_DID}/at.opake.keyring/3abc");

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
        let new_uri = format!("at://{ALICE_DID}/at.opake.keyring/3newsupersede");
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
                assert_eq!(v["collection"], "at.opake.keyring");
                let written: Keyring =
                    serde_json::from_value(v["record"].clone()).expect("record body");
                assert_eq!(written.supersedes.as_deref(), Some(prior_head_uri.as_str()));
                assert_eq!(written.lineage.as_deref(), Some(WORKSPACE_ID));
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
        let prior_head_uri = format!("at://{ALICE_DID}/at.opake.keyring/3abc");

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
        let prior_head_uri = format!("at://{ALICE_DID}/at.opake.keyring/3abc");

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
        let head_uri = format!("at://{ALICE_DID}/at.opake.keyring/3head");
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
        let prior_head_uri = format!("at://{ALICE_DID}/at.opake.keyring/3abc");

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
        let new_uri = format!("at://{BOB_DID}/at.opake.keyring/3leave");
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
                assert_eq!(v["collection"], "at.opake.keyring");
                let written: Keyring =
                    serde_json::from_value(v["record"].clone()).expect("record body");
                assert_eq!(written.supersedes.as_deref(), Some(prior_head_uri.as_str()));
                assert_eq!(written.lineage.as_deref(), Some(WORKSPACE_ID));
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
        let prior_head_uri = format!("at://{ALICE_DID}/at.opake.keyring/3abc");

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
        let prior_head_uri = format!("at://{ALICE_DID}/at.opake.keyring/3abc");

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
        let prior_head_uri = format!("at://{ALICE_DID}/at.opake.keyring/3abc");

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

    const NEW_DID: &str = "did:plc:newjoiner";

    /// A `publicKey/self` record carrying `identity`'s hybrid public keys, so
    /// `resolve_identity` can hand the admitting manager real keys to wrap to.
    fn public_key_response(uri: &str, identity: &Identity) -> HttpResponse {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;
        let record = serde_json::json!({
            "opakeVersion": SCHEMA_VERSION,
            "x25519PublicKey": { "$bytes": b64.encode(identity.x25519_public_key_bytes().unwrap()) },
            "x25519Algo": "x25519",
            "mlKemPublicKey": { "$bytes": b64.encode(identity.ml_kem_public_key_bytes().unwrap()) },
            "mlKemAlgo": "ml-kem-768",
            "createdAt": "2026-03-01T00:00:00Z",
        });
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": uri,
                "cid": "bafypubkey",
                "value": record,
            }))
            .unwrap(),
        }
    }

    /// Admission grants the full history: for every retained rotation the
    /// admitting manager still holds, the joiner's supersede gains a wrapped
    /// copy of that historical key — so documents written under prior
    /// rotations remain readable to a member who joined after them.
    // spec:workspace-key-rotation § New members can read the full history they are admitted to
    #[tokio::test]
    async fn add_member_grants_wrapped_history_to_the_joiner() {
        use crate::crypto::{self, generate_content_key, PrivateKeyBundle, WrapContext};
        use crate::records::KeyHistoryEntry;
        use crate::workspace::HistoricalKey;

        let prior_head_uri = format!("at://{ALICE_DID}/at.opake.keyring/3abc");

        // Prior head at rotation 1 with one retained rotation-0 key in history.
        let mut prior = as_supersede(keyring_with_members(vec![(ALICE_DID, Role::Manager)]));
        prior.rotation = 1;
        prior.key_history.push(KeyHistoryEntry {
            rotation: 0,
            members: prior.members.clone(),
        });
        let genesis = genesis_keyring(vec![(ALICE_DID, Role::Manager)]);

        // The keys the admitting manager holds and extends to the joiner.
        let current_key = generate_content_key(&mut OsRng);
        let historical_key = generate_content_key(&mut OsRng);

        // The joiner's identity — its published keys are what the manager
        // wraps to, and its private keys are what we unwrap with to verify.
        let joiner = Identity::generate(NEW_DID, &mut OsRng);

        let mock = MockTransport::new();
        // fetch_keyring_chain_head
        mock.enqueue(chain_head_response(&prior_head_uri, "bafyhead"));
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.did-plc-alice"));
        mock.enqueue(get_keyring_response(&prior_head_uri, "bafyhead", &prior));
        mock.enqueue(get_keyring_response(WORKSPACE_ID, "bafygenesis", &genesis));
        // resolve_identity(joiner)
        mock.enqueue(did_doc_response(NEW_DID, "https://pds.newjoiner"));
        mock.enqueue(public_key_response(
            &format!("at://{NEW_DID}/at.opake.publicKey/self"),
            &joiner,
        ));
        // supersede write
        let new_uri = format!("at://{ALICE_DID}/at.opake.keyring/3addmember");
        mock.enqueue(create_record_response(&new_uri, "bafynew"));

        let mut opake = opake_for(ALICE_DID, mock.clone());
        let historical_keys = vec![HistoricalKey {
            rotation: 0,
            key: historical_key.clone(),
        }];
        opake
            .add_workspace_member(
                &WorkspaceId::from_resolved(WORKSPACE_ID),
                &current_key,
                &historical_keys,
                NEW_DID,
                Role::Editor,
            )
            .await
            .unwrap();

        // Inspect the written supersede.
        let reqs = mock.requests();
        let create = reqs
            .iter()
            .find(|r| r.url.contains("createRecord"))
            .expect("createRecord");
        let written: Keyring = match &create.body {
            Some(RequestBody::Json(v)) => serde_json::from_value(v["record"].clone()).unwrap(),
            _ => panic!("expected JSON body"),
        };

        // Wraps for this workspace anchor to the genesis URI.
        let ctx = WrapContext::Keyring { uri: WORKSPACE_ID };
        let x25519_priv = joiner.x25519_private_key_bytes().unwrap();
        let ml_kem_priv = joiner.ml_kem_private_key_bytes().unwrap();
        let bundle = PrivateKeyBundle {
            x25519: &x25519_priv,
            ml_kem: &ml_kem_priv,
        };

        // The joiner is a current member and unwraps the current group key.
        let member = written
            .members
            .iter()
            .find(|m| m.did() == NEW_DID)
            .expect("joiner in current members");
        let unwrapped_current =
            crypto::unwrap_key(&member.wrapped_key, &bundle, &ctx, written.opake_version).unwrap();
        assert_eq!(unwrapped_current.0, current_key.0);

        // And the joiner is in the rotation-0 history entry, unwrapping to the
        // retained historical key — the crux of the new-member-history fix.
        let hist_entry = written
            .key_history
            .iter()
            .find(|h| h.rotation == 0)
            .expect("rotation-0 history retained");
        let hist_member = hist_entry
            .members
            .iter()
            .find(|m| m.did() == NEW_DID)
            .expect("joiner granted rotation-0 history wrap");
        let unwrapped_hist = crypto::unwrap_key(
            &hist_member.wrapped_key,
            &bundle,
            &ctx,
            written.opake_version,
        )
        .unwrap();
        assert_eq!(unwrapped_hist.0, historical_key.0);
    }

    /// A member list for a workspace hosted on another PDS was fetched with
    /// a session `getRecord` against the caller's own PDS, which pipethrough-
    /// proxies to an appview that need not exist — so the member list came
    /// back empty or errored for every cross-PDS workspace. The fetch must
    /// resolve the keyring authority's DID document and read the record from
    /// that PDS over the public endpoint, the same route
    /// `resolve_foreign_workspace` takes.
    #[tokio::test]
    #[allow(non_snake_case)] // bug__ regression-naming convention
    async fn bug__member_list_resolves_foreign_authority_pds() {
        const ALICE_PDS: &str = "https://pds.did-plc-alice";

        let keyring_uri = format!("at://{ALICE_DID}/at.opake.keyring/3foreign");
        let keyring =
            keyring_with_members(vec![(ALICE_DID, Role::Manager), (BOB_DID, Role::Editor)]);

        let mock = MockTransport::new();
        mock.enqueue(did_doc_response(ALICE_DID, ALICE_PDS));
        mock.enqueue(get_keyring_response(&keyring_uri, "bafyforeign", &keyring));

        // Bob is the caller; his session PDS is `https://pds.did:plc:bob`.
        let opake = opake_for(BOB_DID, mock.clone());
        let members = opake.workspace_members(&keyring_uri).await;

        // Route first: a session-PDS fetch may fail for its own reasons, and
        // the point of this test is *where* the record was read from.
        let record_request = mock
            .requests()
            .into_iter()
            .find(|r| r.url.contains("getRecord"))
            .expect("keyring record was fetched");

        assert!(
            record_request.url.starts_with(ALICE_PDS),
            "record must be read from the authority's PDS, got {}",
            record_request.url
        );
        assert!(
            !record_request.url.contains(&format!("pds.{BOB_DID}")),
            "record must not be routed through the caller's session PDS, got {}",
            record_request.url
        );
        // The public endpoint is unauthenticated — an auth header would mean
        // the session client, and with it the pipethrough dependency, is back.
        assert!(
            !record_request
                .headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("authorization")),
            "public getRecord must not carry session credentials"
        );

        assert_eq!(
            members
                .expect("member list resolves")
                .iter()
                .map(|m| m.did())
                .collect::<Vec<_>>(),
            vec![ALICE_DID, BOB_DID]
        );
    }

    /// Build a genesis keyring whose group key `gk` is really wrapped to
    /// `member`, addressed at `head_uri`. When `head_uri`'s rkey is
    /// `derive_workspace_identity_tag(gk, authority)` the record is honest;
    /// any other rkey is a forgery that unwraps cleanly but fails identity
    /// derivation. The caller owns `gk` so it can compute the honest rkey.
    fn real_genesis_envelope(
        head_uri: &str,
        member_pub: &crate::crypto::PublicKeyBundle<'_>,
        member_did: &str,
        gk: &crate::crypto::ContentKey,
        rng: &mut crate::crypto::OsRng,
    ) -> crate::indexer::types::IndexerEnvelope<crate::records::Keyring> {
        use crate::crypto::{
            encrypt_metadata, wrap_key, KeyringMetadata, SealContext, SealType, WrapContext,
        };
        use crate::records::{Keyring, KeyringMember, Role, SCHEMA_VERSION};

        let wrapped = wrap_key(
            gk,
            member_pub,
            member_did,
            &WrapContext::Keyring { uri: head_uri },
            rng,
        )
        .unwrap();
        let meta_ctx = SealContext::new(head_uri, SealType::KeyringMetadata);
        let encrypted_metadata = encrypt_metadata(
            gk,
            &KeyringMetadata {
                name: "W".into(),
                description: None,
                icon: None,
            },
            &meta_ctx,
            rng,
        )
        .unwrap();
        crate::indexer::types::IndexerEnvelope {
            uri: head_uri.to_string(),
            record: Keyring {
                opake_version: SCHEMA_VERSION,
                algo: "aes-256-gcm".into(),
                members: vec![KeyringMember {
                    wrapped_key: wrapped,
                    role: Role::Manager,
                }],
                rotation: 0,
                key_history: Vec::new(),
                encrypted_metadata,
                supersedes: None,
                supersedes_cid: None,
                lineage: None,
                created_at: "2026-04-17T00:00:00Z".into(),
                modified_at: None,
            },
            indexed_at: "2026-04-17T00:00:01Z".into(),
            deleted_at: None,
        }
    }

    /// C1 regression: the CLI daemon's sync loop is the sole workspace-identity
    /// adoption surface on a keeper-less client. A keyring that unwraps for the
    /// caller but whose declared genesis rkey is not derived from its key
    /// material must be rejected before any tree sync — the same forgery the
    /// WASM keeper drops. Without the check the daemon walks and adopts a
    /// foreign identity under the attacker's group key.
    // spec: workspace-identity § Identity adoption verifies by derivation
    #[tokio::test]
    #[allow(non_snake_case)] // bug__ regression-naming convention
    async fn bug__daemon_sync_rejects_forged_workspace_identity() {
        let mock = MockTransport::new();
        let mut opake = opake_for(BOB_DID, mock.clone());

        let x = opake.identity().x25519_public_key_bytes().unwrap();
        let m = opake.identity().ml_kem_public_key_bytes().unwrap();
        let bob_pub = crate::crypto::PublicKeyBundle {
            x25519: &x,
            ml_kem: &m,
        };
        let mut rng = crate::crypto::OsRng;

        // Forged: an arbitrary rkey Bob's key cannot derive.
        let gk = crate::crypto::generate_content_key(&mut rng);
        let forged_uri = format!("at://{ALICE_DID}/at.opake.keyring/forgednotaderivedtag00000");
        let forged = real_genesis_envelope(&forged_uri, &bob_pub, BOB_DID, &gk, &mut rng);

        let private_keys = opake.identity().owned_private_keys().unwrap();
        let result = opake
            .sync_single_workspace(&forged, &private_keys.bundle())
            .await;

        assert!(
            result
                .error
                .as_deref()
                .is_some_and(|e| e.contains("identity could not be verified")),
            "forged workspace identity must be rejected, got {:?}",
            result.error
        );
        assert!(
            mock.requests().is_empty(),
            "rejection must short-circuit before any tree fetch"
        );
    }

    /// Contrast for the C1 regression: an honestly derived genesis rkey passes
    /// the identity gate, so the sync proceeds to `load_tree` and any error is
    /// a tree-sync error — never an identity mismatch. Proves the gate admits
    /// legitimate workspaces rather than rejecting everything.
    // spec: workspace-identity § Identity adoption verifies by derivation
    #[tokio::test]
    async fn daemon_sync_admits_derived_workspace_identity() {
        let mock = MockTransport::new();
        let mut opake = opake_for(BOB_DID, mock.clone());

        let x = opake.identity().x25519_public_key_bytes().unwrap();
        let m = opake.identity().ml_kem_public_key_bytes().unwrap();
        let bob_pub = crate::crypto::PublicKeyBundle {
            x25519: &x,
            ml_kem: &m,
        };
        let mut rng = crate::crypto::OsRng;

        // Honest by construction: the rkey IS the tag derived from this gk.
        let gk = crate::crypto::generate_content_key(&mut rng);
        let tag = crate::crypto::derive_workspace_identity_tag(&gk, ALICE_DID);
        let honest_uri = format!("at://{ALICE_DID}/at.opake.keyring/{tag}");
        let honest = real_genesis_envelope(&honest_uri, &bob_pub, BOB_DID, &gk, &mut rng);

        let private_keys = opake.identity().owned_private_keys().unwrap();
        let result = opake
            .sync_single_workspace(&honest, &private_keys.bundle())
            .await;

        // The gate passed: any error is from the (unmocked) tree fetch, not
        // from identity verification.
        if let Some(err) = result.error.as_deref() {
            assert!(
                !err.contains("identity could not be verified"),
                "honest derived identity must pass the gate, got {err}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Name resolution across the visibility gap
// ---------------------------------------------------------------------------

mod workspace_resolution {
    use super::*;
    use crate::client::HttpResponse;
    use crate::crypto::{encrypt_metadata, wrap_key, KeyringMetadata, WrapContext};
    use crate::indexer::retry::{MAX_DELAY_MS, MAX_WINDOW_MS};
    use crate::records::{Keyring, KeyringMember, Role, SCHEMA_VERSION};
    use std::sync::atomic::{AtomicU64, Ordering};

    const DID: &str = "did:plc:test";
    const INDEXER_URL: &str = "https://indexer.test";
    const WORKSPACE_URI: &str = "at://did:plc:test/at.opake.keyring/genesis";

    /// The exhaustion test needs a clock that actually advances; `fn() -> u64`
    /// carries no state, so the tick lives in a static that only that test's
    /// sleeper writes.
    static EXHAUSTION_CLOCK_MICROS: AtomicU64 = AtomicU64::new(0);

    fn exhaustion_now_micros() -> u64 {
        EXHAUSTION_CLOCK_MICROS.load(Ordering::SeqCst)
    }

    /// `/api/keyrings` answering with the given workspaces.
    fn keyrings_response(workspaces: Vec<serde_json::Value>) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({ "workspaces": workspaces })).unwrap(),
        }
    }

    /// A listing envelope for a workspace named `name`, wrapped to `identity`
    /// so `resolve_workspace_uri`'s name match decrypts it for real.
    fn named_workspace(identity: &Identity, name: &str) -> serde_json::Value {
        let group_key = generate_content_key(&mut OsRng);
        let public_keys = identity.owned_public_keys().unwrap();
        let wrapped = wrap_key(
            &group_key,
            &public_keys.bundle(),
            DID,
            &WrapContext::Keyring { uri: WORKSPACE_URI },
            &mut OsRng,
        )
        .unwrap();
        let meta_context = crate::crypto::SealContext::new(
            WORKSPACE_URI,
            crate::crypto::SealType::KeyringMetadata,
        );
        let encrypted_metadata = encrypt_metadata(
            &group_key,
            &KeyringMetadata {
                name: name.into(),
                description: None,
                icon: None,
            },
            &meta_context,
            &mut OsRng,
        )
        .unwrap();

        let keyring = Keyring {
            opake_version: SCHEMA_VERSION,
            algo: "aes-256-gcm".into(),
            members: vec![KeyringMember {
                wrapped_key: wrapped,
                role: Role::Manager,
            }],
            rotation: 0,
            key_history: Vec::new(),
            encrypted_metadata,
            supersedes: None,
            supersedes_cid: None,
            lineage: None,
            created_at: "2026-07-14T00:00:00Z".into(),
            modified_at: None,
        };

        serde_json::json!({
            "uri": WORKSPACE_URI,
            "record": keyring,
            "indexedAt": "2026-07-14T00:00:01Z",
        })
    }

    fn opake_with(
        mock: MockTransport,
        now_micros: fn() -> u64,
        sleep: crate::indexer::retry::SleepFn,
    ) -> Opake<MockTransport, OsRng, NoopStorage> {
        let client = crate::client::XrpcClient::new(mock, "https://pds.example.com".into());
        let identity = Identity::generate(DID, &mut OsRng);
        let mut opake =
            Opake::new(client, DID.into(), identity, OsRng, NoopStorage, now_micros).unwrap();
        opake.set_indexer_url(INDEXER_URL.into());
        opake.set_sleep_fn(sleep);
        opake
    }

    /// The CLI's create-then-mutate: `workspace create X` followed immediately
    /// by a mutation of X, with the indexer's listing not yet carrying the
    /// genesis keyring. The first listing must be absorbed, not surfaced.
    // spec:indexer-consistency § Dependent operations tolerate the visibility gap
    #[tokio::test]
    async fn name_resolution_retries_until_the_workspace_is_listed() {
        let mock = MockTransport::new();
        // Genesis not consumed yet, then consumed.
        mock.enqueue(keyrings_response(vec![]));

        let mut opake = opake_with(
            mock.clone(),
            test_now_micros,
            Box::new(|_| Box::pin(async {})),
        );
        mock.enqueue(keyrings_response(vec![named_workspace(
            opake.identity(),
            "fresh-name",
        )]));

        let uri = opake.resolve_workspace_uri("fresh-name").await.unwrap();

        assert_eq!(uri, WORKSPACE_URI);
        let listings = mock
            .requests()
            .iter()
            .filter(|r| r.url.contains("/api/keyrings"))
            .count();
        assert_eq!(listings, 2, "the first listing must be retried, not failed");
    }

    /// Window exhaustion names the workspace that was awaited — the same
    /// convention the chain-head wait follows, so a name that never lands is
    /// legible in the failure instead of reading as an anonymous timeout.
    // spec:indexer-consistency § Dependent operations tolerate the visibility gap
    #[tokio::test]
    async fn name_resolution_exhaustion_names_the_workspace() {
        EXHAUSTION_CLOCK_MICROS.store(0, Ordering::SeqCst);

        let mock = MockTransport::new();
        // The schedule fits a handful of attempts in the window; queue well
        // past that so exhaustion, not an empty mock queue, ends the loop.
        for _ in 0..32 {
            mock.enqueue(keyrings_response(vec![]));
        }

        let sleep: crate::indexer::retry::SleepFn = Box::new(|delay| {
            EXHAUSTION_CLOCK_MICROS.fetch_add((delay.as_millis() as u64) * 1_000, Ordering::SeqCst);
            Box::pin(async {})
        });
        let mut opake = opake_with(mock, exhaustion_now_micros, sleep);

        let error = opake
            .resolve_workspace_uri("never-lands")
            .await
            .unwrap_err();

        match error {
            Error::VisibilityTimeout {
                ref operation,
                waited_ms,
            } => {
                assert!(
                    operation.contains("never-lands"),
                    "the timeout must name the workspace it waited on: {operation}"
                );
                assert!(waited_ms >= MAX_WINDOW_MS - MAX_DELAY_MS);
            }
            other => panic!("expected VisibilityTimeout, got {other:?}"),
        }
    }
}
