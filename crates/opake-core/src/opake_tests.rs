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
                .map(|(did, role)| {
                    KeyringMember::with_wrap(
                        WrappedKey {
                            did: did.into(),
                            ciphertext: AtBytes {
                                encoded: "AAAA".into(),
                            },
                            algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
                        },
                        role,
                    )
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
        opake_for_identity(did, Identity::generate(did, &mut OsRng), mock)
    }

    fn opake_for_identity(
        did: &str,
        identity: Identity,
        mock: MockTransport,
    ) -> Opake<MockTransport, OsRng, NoopStorage> {
        let session = Session::Legacy(LegacySession {
            did: did.into(),
            handle: "test.handle".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        let client = XrpcClient::with_session(mock, format!("https://pds.{did}"), session);
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

    /// Construct a real rotation-0 keyring fixture.  The URI is derived from
    /// the group key, every member wrap is bound to that URI, and metadata is
    /// encrypted under the same key.  Membership mutation tests must not use
    /// placeholder wraps: production checks unwrap the live head before a
    /// mutation and removal decrypts/re-encrypts this metadata.
    fn real_genesis_keyring(
        owner_did: &str,
        members: Vec<(&str, Role, &Identity)>,
    ) -> (String, crate::crypto::ContentKey, Keyring) {
        use crate::crypto::{
            encrypt_metadata, generate_content_key, wrap_key, KeyringMetadata, PublicKeyBundle,
            SealContext, SealType, WrapContext,
        };

        let mut rng = OsRng;
        let group_key = generate_content_key(&mut rng);
        let tag = crate::crypto::derive_workspace_identity_tag(&group_key, owner_did);
        let workspace_id = format!("at://{owner_did}/at.opake.keyring/{tag}");
        let members = members
            .into_iter()
            .map(|(did, role, identity)| {
                let x25519 = identity.x25519_public_key_bytes().unwrap();
                let ml_kem = identity.ml_kem_public_key_bytes().unwrap();
                let wrapped = wrap_key(
                    &group_key,
                    &PublicKeyBundle {
                        x25519: &x25519,
                        ml_kem: &ml_kem,
                    },
                    did,
                    &WrapContext::Keyring { uri: &workspace_id },
                    &mut rng,
                )
                .unwrap();
                KeyringMember::with_wrap(wrapped, role)
            })
            .collect();
        let metadata = encrypt_metadata(
            &group_key,
            &KeyringMetadata {
                name: "fixture workspace".into(),
                description: Some("real encrypted fixture".into()),
                icon: None,
            },
            &SealContext::new(&workspace_id, SealType::KeyringMetadata),
            &mut rng,
        )
        .unwrap();
        (
            workspace_id,
            group_key,
            Keyring::new(members, metadata, "2026-09-12T00:00:00Z".into()),
        )
    }

    /// Build a genuine later head for repair tests: its current wraps and
    /// metadata use `current_key`, while rotation 0 preserves the original
    /// genesis members with their independently usable wraps.
    fn real_rotated_head(
        workspace_uri: &str,
        genesis: &Keyring,
        current_key: &crate::crypto::ContentKey,
        members: Vec<(&str, Role, &Identity)>,
    ) -> Keyring {
        use crate::crypto::{
            encrypt_metadata, wrap_key, KeyringMetadata, PublicKeyBundle, SealContext, SealType,
            WrapContext,
        };

        let mut rng = OsRng;
        let members = members
            .into_iter()
            .map(|(did, role, identity)| {
                let x25519 = identity.x25519_public_key_bytes().unwrap();
                let ml_kem = identity.ml_kem_public_key_bytes().unwrap();
                KeyringMember::with_wrap(
                    wrap_key(
                        current_key,
                        &PublicKeyBundle {
                            x25519: &x25519,
                            ml_kem: &ml_kem,
                        },
                        did,
                        &WrapContext::Keyring { uri: workspace_uri },
                        &mut rng,
                    )
                    .unwrap(),
                    role,
                )
            })
            .collect();
        Keyring {
            opake_version: genesis.opake_version,
            algo: genesis.algo.clone(),
            members,
            // Skip rotation 1 deliberately: a repair at rotation 2 must not
            // pretend it supplied a generation the member never received.
            rotation: 2,
            key_history: vec![crate::records::KeyHistoryEntry {
                rotation: genesis.rotation,
                members: genesis.members.clone(),
            }],
            encrypted_metadata: encrypt_metadata(
                current_key,
                &KeyringMetadata {
                    name: "fixture workspace rotation 2".into(),
                    description: None,
                    icon: None,
                },
                &SealContext::new(workspace_uri, SealType::KeyringMetadata),
                &mut rng,
            )
            .unwrap(),
            supersedes: Some(workspace_uri.into()),
            supersedes_cid: None,
            lineage: Some(workspace_uri.into()),
            created_at: "2026-09-12T00:00:01Z".into(),
            modified_at: Some("2026-09-12T00:00:01Z".into()),
        }
    }

    /// Queue the exact indexer/DID/PDS chain walk performed by membership
    /// mutations.  A same-rotation head has a real genesis predecessor;
    /// genesis-only fixtures use the same record for both inputs.
    fn enqueue_real_head(
        mock: &MockTransport,
        workspace_id: &str,
        head_uri: &str,
        head_cid: &str,
        head: &Keyring,
        genesis: &Keyring,
    ) {
        mock.enqueue(chain_head_response(head_uri, head_cid));
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.did-plc-alice"));
        mock.enqueue(get_keyring_response(head_uri, head_cid, head));
        if head_uri != workspace_id {
            mock.enqueue(get_keyring_response(workspace_id, "bafygenesis", genesis));
        }
    }

    fn anchored_did_doc_response(did: &str, pds_url: &str, anchor: &[u8; 32]) -> HttpResponse {
        let multibase = crate::client::encode_ed25519_did_key(anchor);
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
                }],
                "verificationMethod": [{
                    "id": format!("{did}#opake"),
                    "controller": did,
                    "type": "Multikey",
                    "publicKeyMultibase": multibase.strip_prefix("did:key:").unwrap(),
                }],
            }))
            .unwrap(),
        }
    }

    fn written_keyring(mock: &MockTransport) -> Keyring {
        let create = mock
            .requests()
            .into_iter()
            .find(|request| request.url.contains("createRecord"))
            .expect("keyring supersede write");
        match create.body {
            Some(RequestBody::Json(body)) => {
                serde_json::from_value(body["record"].clone()).unwrap()
            }
            _ => panic!("keyring supersede must carry a JSON record"),
        }
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

    fn public_key_response_for_bundle(uri: &str, x25519: &[u8], ml_kem: &[u8]) -> HttpResponse {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": uri,
                "cid": "bafypubkey",
                "value": {
                    "opakeVersion": SCHEMA_VERSION,
                    "x25519PublicKey": { "$bytes": b64.encode(x25519) },
                    "x25519Algo": "x25519",
                    "mlKemPublicKey": { "$bytes": b64.encode(ml_kem) },
                    "mlKemAlgo": "ml-kem-768",
                    "createdAt": "2026-03-01T00:00:00Z",
                },
            }))
            .unwrap(),
        }
    }

    /// Admission grants the full history: for every retained rotation the
    /// admitting manager still holds, the joiner's supersede gains a wrapped
    /// copy of that historical key — so documents written under prior
    /// rotations remain readable to a member who joined after them.
    // spec:key-rotation § New members can read the full history they are admitted to
    #[tokio::test]
    async fn add_member_grants_wrapped_history_to_the_joiner() {
        use crate::crypto::{self, generate_content_key, PrivateKeyBundle, WrapContext};
        use crate::records::KeyHistoryEntry;
        use crate::workspace::HistoricalKey;

        let prior_head_uri = format!("at://{ALICE_DID}/at.opake.keyring/3abc");

        // The keys the admitting manager holds and extends to the joiner.
        let current_key = generate_content_key(&mut OsRng);
        let historical_key = generate_content_key(&mut OsRng);

        let mock = MockTransport::new();
        let mut opake = opake_for(ALICE_DID, mock.clone());
        let alice = opake.identity();
        let alice_public_keys = crate::crypto::PublicKeyBundle {
            x25519: &alice.x25519_public_key_bytes().unwrap(),
            ml_kem: &alice.ml_kem_public_key_bytes().unwrap(),
        };
        let context = WrapContext::Keyring { uri: WORKSPACE_ID };
        let current_wrap = crypto::wrap_key(
            &current_key,
            &alice_public_keys,
            ALICE_DID,
            &context,
            &mut OsRng,
        )
        .unwrap();
        let historical_wrap = crypto::wrap_key(
            &historical_key,
            &alice_public_keys,
            ALICE_DID,
            &context,
            &mut OsRng,
        )
        .unwrap();

        // Prior head at rotation 1 with real current and rotation-0 wraps
        // for the manager. Admission validates the live current head before
        // it will use a caller-supplied group key.
        let mut prior = as_supersede(keyring_with_members(vec![(ALICE_DID, Role::Manager)]));
        prior.members = vec![KeyringMember::with_wrap(current_wrap, Role::Manager)];
        prior.rotation = 1;
        prior.key_history.push(KeyHistoryEntry {
            rotation: 0,
            members: vec![KeyringMember::with_wrap(historical_wrap, Role::Manager)],
        });
        let genesis = genesis_keyring(vec![(ALICE_DID, Role::Manager)]);

        // The joiner's identity — its published keys are what the manager
        // wraps to, and its private keys are what we unwrap with to verify.
        let joiner = Identity::generate(NEW_DID, &mut OsRng);

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
                Some(crate::crypto::unverified_key_approval(
                    SCHEMA_VERSION,
                    WORKSPACE_ID,
                    NEW_DID,
                    &crate::crypto::EncryptionKeyFields {
                        x25519_public_key: &joiner.x25519_public_key_bytes().unwrap(),
                        x25519_algo: "x25519",
                        ml_kem_public_key: &joiner.ml_kem_public_key_bytes().unwrap(),
                        ml_kem_algo: "ml-kem-768",
                    },
                )),
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
        let unwrapped_current = crypto::unwrap_key(
            member.wrapped_key.as_ref().unwrap(),
            &bundle,
            &ctx,
            written.opake_version,
        )
        .unwrap();
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
            hist_member.wrapped_key.as_ref().unwrap(),
            &bundle,
            &ctx,
            written.opake_version,
        )
        .unwrap();
        assert_eq!(unwrapped_hist.0, historical_key.0);
    }

    // spec:workspace-membership § Adding a member is a manager-authored supersede
    #[tokio::test]
    async fn add_member_refuses_verification_failure_before_any_supersede_write() {
        let alice = Identity::generate(ALICE_DID, &mut OsRng);
        let recipient = Identity::generate(NEW_DID, &mut OsRng);
        let (workspace_uri, group_key, genesis) =
            real_genesis_keyring(ALICE_DID, vec![(ALICE_DID, Role::Manager, &alice)]);
        let workspace_id = WorkspaceId::from_resolved(workspace_uri.clone());
        let mock = MockTransport::new();
        enqueue_real_head(
            &mock,
            &workspace_uri,
            &workspace_uri,
            "bafygenesis",
            &genesis,
            &genesis,
        );

        // An #opake method makes an unsigned public-key record a hard
        // verification failure, rather than a confirmation candidate.
        let anchor = crate::crypto::Ed25519SigningKey::from_bytes(&[71; 32])
            .verifying_key()
            .to_bytes();
        mock.enqueue(anchored_did_doc_response(
            NEW_DID,
            "https://pds.newjoiner",
            &anchor,
        ));
        mock.enqueue(public_key_response(
            &format!("at://{NEW_DID}/at.opake.publicKey/self"),
            &recipient,
        ));

        let mut opake = opake_for_identity(ALICE_DID, alice, mock.clone());
        let err = opake
            .add_workspace_member(&workspace_id, &group_key, &[], NEW_DID, Role::Editor, None)
            .await
            .unwrap_err();
        assert!(matches!(err, crate::error::Error::VerificationFailed(_)));
        assert!(
            !mock
                .requests()
                .iter()
                .any(|request| request.url.contains("createRecord")),
            "a verification failure must be refused before any keyring write"
        );
    }

    fn approval_for_unverified_member(
        workspace_uri: &str,
        member_did: &str,
        identity: &Identity,
    ) -> [u8; 32] {
        crate::crypto::unverified_key_approval(
            SCHEMA_VERSION,
            workspace_uri,
            member_did,
            &crate::crypto::EncryptionKeyFields {
                x25519_public_key: &identity.x25519_public_key_bytes().unwrap(),
                x25519_algo: "x25519",
                ml_kem_public_key: &identity.ml_kem_public_key_bytes().unwrap(),
                ml_kem_algo: "ml-kem-768",
            },
        )
    }

    // spec:workspace-membership § Membership state is the keyring head's member list
    #[tokio::test]
    async fn manager_without_current_wrap_can_approve_unverified_member_without_re_admission() {
        let alice = Identity::generate(ALICE_DID, &mut OsRng);
        let carol_did = "did:plc:carol";
        let carol = Identity::generate(carol_did, &mut OsRng);
        let (workspace_uri, _, mut head) = real_genesis_keyring(
            ALICE_DID,
            vec![
                (ALICE_DID, Role::Manager, &alice),
                (carol_did, Role::Editor, &carol),
            ],
        );
        head.members
            .iter_mut()
            .find(|member| member.did() == carol_did)
            .unwrap()
            .wrapped_key = None;
        // Approval is a manager-authorized head mutation, not a group-key
        // operation. A manager retained with historical access can therefore
        // record consent for another manager to repair later.
        head.members
            .iter_mut()
            .find(|member| member.did() == ALICE_DID)
            .unwrap()
            .wrapped_key = None;
        let approval = approval_for_unverified_member(&workspace_uri, carol_did, &carol);
        let workspace_id = WorkspaceId::from_resolved(workspace_uri.clone());
        let mock = MockTransport::new();
        enqueue_real_head(
            &mock,
            &workspace_uri,
            &workspace_uri,
            "bafyhead",
            &head,
            &head,
        );
        mock.enqueue(did_doc_response(carol_did, "https://pds.carol"));
        mock.enqueue(public_key_response(
            &format!("at://{carol_did}/at.opake.publicKey/self"),
            &carol,
        ));
        // Approval resolution is network I/O; the live head is checked again
        // immediately before the same-rotation supersede is written.
        enqueue_real_head(
            &mock,
            &workspace_uri,
            &workspace_uri,
            "bafyhead",
            &head,
            &head,
        );
        mock.enqueue(create_record_response(
            &format!("at://{ALICE_DID}/at.opake.keyring/3approval"),
            "bafyapproval",
        ));

        let mut opake = opake_for_identity(ALICE_DID, alice, mock.clone());
        opake
            .approve_pending_workspace_member(&workspace_id, carol_did, approval)
            .await
            .unwrap();

        let written = written_keyring(&mock);
        let carol_entry = written
            .members
            .iter()
            .find(|member| member.did() == carol_did)
            .unwrap();
        assert!(carol_entry.wrapped_key.is_none());
        assert_eq!(
            carol_entry
                .unverified_key_approval
                .as_ref()
                .unwrap()
                .decode()
                .unwrap(),
            approval
        );
        assert_eq!(written.rotation, head.rotation);
    }

    // spec:workspace-membership § Membership state is the keyring head's member list
    #[tokio::test]
    async fn restored_head_does_not_borrow_approval_from_deleted_repair() {
        let alice = Identity::generate(ALICE_DID, &mut OsRng);
        let carol_did = "did:plc:carol";
        let carol = Identity::generate(carol_did, &mut OsRng);
        let (workspace_uri, _, mut restored_head) = real_genesis_keyring(
            ALICE_DID,
            vec![
                (ALICE_DID, Role::Manager, &alice),
                (carol_did, Role::Editor, &carol),
            ],
        );
        restored_head
            .members
            .iter_mut()
            .find(|member| member.did() == carol_did)
            .unwrap()
            .wrapped_key = None;

        // A subsequently deleted repair had a matching approval, but it is
        // not the live head and must contribute nothing to a fresh status.
        let mut deleted_repair = restored_head.clone();
        deleted_repair
            .members
            .iter_mut()
            .find(|member| member.did() == carol_did)
            .unwrap()
            .unverified_key_approval = Some(AtBytes::from_raw(&approval_for_unverified_member(
            &workspace_uri,
            carol_did,
            &carol,
        )));

        let workspace_id = WorkspaceId::from_resolved(workspace_uri.clone());
        let mock = MockTransport::new();
        enqueue_real_head(
            &mock,
            &workspace_uri,
            &workspace_uri,
            "bafyrestored",
            &restored_head,
            &restored_head,
        );
        mock.enqueue(did_doc_response(carol_did, "https://pds.carol"));
        mock.enqueue(public_key_response(
            &format!("at://{carol_did}/at.opake.publicKey/self"),
            &carol,
        ));

        let mut opake = opake_for_identity(ALICE_DID, alice, mock.clone());
        let status = opake
            .workspace_member_access_status(&workspace_id, carol_did)
            .await
            .unwrap();

        assert!(!status.has_current_wrap);
        assert_eq!(
            status.verification,
            MemberVerificationStatus::UnverifiedApprovalRequired
        );
        assert!(status.can_repair);
        assert!(
            mock.requests()
                .iter()
                .all(|request| !request.url.contains("repair")),
            "the inspection reads only the restored live head, never a deleted repair"
        );
    }

    // spec:account-verification § Key-bound approval is carried by the relationship's records
    #[tokio::test]
    async fn approval_refuses_either_unverified_encryption_key_substitution() {
        let carol_did = "did:plc:carol";
        for changed_half in ["x25519", "ml-kem"] {
            let alice = Identity::generate(ALICE_DID, &mut OsRng);
            let approved = Identity::generate(carol_did, &mut OsRng);
            let replacement = Identity::generate(carol_did, &mut OsRng);
            let (workspace_uri, _, mut head) = real_genesis_keyring(
                ALICE_DID,
                vec![
                    (ALICE_DID, Role::Manager, &alice),
                    (carol_did, Role::Editor, &approved),
                ],
            );
            head.members
                .iter_mut()
                .find(|member| member.did() == carol_did)
                .unwrap()
                .wrapped_key = None;
            let approval = approval_for_unverified_member(&workspace_uri, carol_did, &approved);
            let workspace_id = WorkspaceId::from_resolved(workspace_uri.clone());
            let mock = MockTransport::new();
            enqueue_real_head(
                &mock,
                &workspace_uri,
                &workspace_uri,
                "bafyhead",
                &head,
                &head,
            );
            mock.enqueue(did_doc_response(carol_did, "https://pds.carol"));
            let approved_x = approved.x25519_public_key_bytes().unwrap();
            let approved_ml = approved.ml_kem_public_key_bytes().unwrap();
            let replacement_x = replacement.x25519_public_key_bytes().unwrap();
            let replacement_ml = replacement.ml_kem_public_key_bytes().unwrap();
            mock.enqueue(public_key_response_for_bundle(
                &format!("at://{carol_did}/at.opake.publicKey/self"),
                if changed_half == "x25519" {
                    &replacement_x
                } else {
                    &approved_x
                },
                if changed_half == "ml-kem" {
                    &replacement_ml
                } else {
                    &approved_ml
                },
            ));

            let mut opake = opake_for_identity(ALICE_DID, alice, mock.clone());
            let err = opake
                .approve_pending_workspace_member(&workspace_id, carol_did, approval)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                crate::error::Error::UnverifiedKeyApprovalRequired { .. }
            ));
            assert!(
                !mock
                    .requests()
                    .iter()
                    .any(|request| request.url.contains("createRecord")),
                "{changed_half} substitution must not write an approval"
            );
        }
    }

    // spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    #[tokio::test]
    async fn non_manager_cannot_approve_or_repair_another_members_wrap() {
        let alice = Identity::generate(ALICE_DID, &mut OsRng);
        let bob = Identity::generate(BOB_DID, &mut OsRng);
        let carol_did = "did:plc:carol";
        let carol = Identity::generate(carol_did, &mut OsRng);
        let (workspace_uri, group_key, mut head) = real_genesis_keyring(
            ALICE_DID,
            vec![
                (ALICE_DID, Role::Manager, &alice),
                (BOB_DID, Role::Editor, &bob),
                (carol_did, Role::Editor, &carol),
            ],
        );
        head.members
            .iter_mut()
            .find(|member| member.did() == carol_did)
            .unwrap()
            .wrapped_key = None;
        let workspace_id = WorkspaceId::from_resolved(workspace_uri.clone());
        let mock = MockTransport::new();
        enqueue_real_head(
            &mock,
            &workspace_uri,
            &workspace_uri,
            "bafyhead",
            &head,
            &head,
        );
        let mut opake = opake_for_identity(BOB_DID, bob, mock.clone());
        let err = opake
            .repair_workspace_member_wrap(&workspace_id, &group_key, carol_did, None)
            .await
            .unwrap_err();
        assert!(matches!(err, crate::error::Error::Auth(_)));
        assert!(
            !mock
                .requests()
                .iter()
                .any(|request| request.url.contains("createRecord")),
            "a non-manager must not write a repair"
        );
    }

    // spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    #[tokio::test]
    async fn same_rotation_repair_wraps_only_the_pending_member_and_preserves_the_head() {
        use crate::crypto::{PrivateKeyBundle, WrapContext};

        let alice = Identity::generate(ALICE_DID, &mut OsRng);
        let bob = Identity::generate(BOB_DID, &mut OsRng);
        let carol_did = "did:plc:carol";
        let carol = Identity::generate(carol_did, &mut OsRng);
        let (workspace_uri, _, genesis) = real_genesis_keyring(
            ALICE_DID,
            vec![
                (ALICE_DID, Role::Manager, &alice),
                (BOB_DID, Role::Editor, &bob),
                (carol_did, Role::Editor, &carol),
            ],
        );
        let group_key = crate::crypto::generate_content_key(&mut OsRng);
        let mut head = real_rotated_head(
            &workspace_uri,
            &genesis,
            &group_key,
            vec![
                (ALICE_DID, Role::Manager, &alice),
                (BOB_DID, Role::Editor, &bob),
                (carol_did, Role::Editor, &carol),
            ],
        );
        let approval = approval_for_unverified_member(&workspace_uri, carol_did, &carol);
        let pending = head
            .members
            .iter_mut()
            .find(|member| member.did() == carol_did)
            .unwrap();
        pending.wrapped_key = None;
        pending.unverified_key_approval = Some(AtBytes::from_raw(&approval));
        let bob_before = serde_json::to_value(
            head.members
                .iter()
                .find(|member| member.did() == BOB_DID)
                .unwrap(),
        )
        .unwrap();
        let workspace_id = WorkspaceId::from_resolved(workspace_uri.clone());
        let head_uri = format!("at://{ALICE_DID}/at.opake.keyring/3rotation2");
        let mock = MockTransport::new();
        enqueue_real_head(
            &mock,
            &workspace_uri,
            &head_uri,
            "bafyhead",
            &head,
            &genesis,
        );
        mock.enqueue(did_doc_response(carol_did, "https://pds.carol"));
        mock.enqueue(public_key_response(
            &format!("at://{carol_did}/at.opake.publicKey/self"),
            &carol,
        ));
        enqueue_real_head(
            &mock,
            &workspace_uri,
            &head_uri,
            "bafyhead",
            &head,
            &genesis,
        );
        mock.enqueue(create_record_response(
            &format!("at://{ALICE_DID}/at.opake.keyring/3repair"),
            "bafyrepair",
        ));

        let mut opake = opake_for_identity(ALICE_DID, alice, mock.clone());
        opake
            .repair_workspace_member_wrap(&workspace_id, &group_key, carol_did, None)
            .await
            .unwrap();
        let written = written_keyring(&mock);
        assert_eq!(written.rotation, head.rotation, "repair is same-rotation");
        assert_eq!(
            serde_json::to_value(&written.key_history).unwrap(),
            serde_json::to_value(&head.key_history).unwrap(),
            "repair preserves the real historical wraps verbatim"
        );
        assert!(
            written.key_history.iter().all(|entry| entry.rotation != 1),
            "repair at rotation 2 must not claim the missing rotation 1 was repaired"
        );
        assert_eq!(
            serde_json::to_value(
                written
                    .members
                    .iter()
                    .find(|member| member.did() == BOB_DID)
                    .unwrap(),
            )
            .unwrap(),
            bob_before,
            "unrelated member entry must be carried verbatim"
        );
        let carol_wrap = written
            .members
            .iter()
            .find(|member| member.did() == carol_did)
            .unwrap()
            .wrapped_key
            .as_ref()
            .expect("repair supplies only Carol's missing current wrap");
        let x25519 = carol.x25519_private_key_bytes().unwrap();
        let ml_kem = carol.ml_kem_private_key_bytes().unwrap();
        let unwrapped = crate::crypto::unwrap_key(
            carol_wrap,
            &PrivateKeyBundle {
                x25519: &x25519,
                ml_kem: &ml_kem,
            },
            &WrapContext::Keyring {
                uri: &workspace_uri,
            },
            written.opake_version,
        )
        .unwrap();
        assert_eq!(unwrapped.0, group_key.0);
    }

    // spec:key-rotation § The rotation event is synchronous and self-sufficient
    #[tokio::test]
    async fn same_rotation_repair_rejects_stale_head_without_readding_removed_member() {
        let alice = Identity::generate(ALICE_DID, &mut OsRng);
        let bob = Identity::generate(BOB_DID, &mut OsRng);
        let carol_did = "did:plc:carol";
        let carol = Identity::generate(carol_did, &mut OsRng);
        let (workspace_uri, group_key, mut prior) = real_genesis_keyring(
            ALICE_DID,
            vec![
                (ALICE_DID, Role::Manager, &alice),
                (BOB_DID, Role::Editor, &bob),
                (carol_did, Role::Editor, &carol),
            ],
        );
        let approval = approval_for_unverified_member(&workspace_uri, carol_did, &carol);
        let pending = prior
            .members
            .iter_mut()
            .find(|member| member.did() == carol_did)
            .unwrap();
        pending.wrapped_key = None;
        pending.unverified_key_approval = Some(AtBytes::from_raw(&approval));
        let workspace_id = WorkspaceId::from_resolved(workspace_uri.clone());
        let stale_head_uri = format!("at://{ALICE_DID}/at.opake.keyring/3pending");
        let mut stale_head = prior.clone();
        stale_head.supersedes = Some(workspace_uri.clone());
        stale_head.lineage = Some(workspace_uri.clone());
        let mock = MockTransport::new();
        enqueue_real_head(
            &mock,
            &workspace_uri,
            &stale_head_uri,
            "bafystale",
            &stale_head,
            &prior,
        );
        mock.enqueue(did_doc_response(carol_did, "https://pds.carol"));
        mock.enqueue(public_key_response(
            &format!("at://{carol_did}/at.opake.publicKey/self"),
            &carol,
        ));
        // Carol was removed while the public-key lookup was in flight.  The
        // second chain resolution returns that newer, real head; repair must
        // fail rather than overwrite it or re-add Carol from `stale_head`.
        let removed_head_uri = format!("at://{ALICE_DID}/at.opake.keyring/3removed");
        let mut removed_head = stale_head.clone();
        removed_head
            .members
            .retain(|member| member.did() != carol_did);
        removed_head.supersedes = Some(workspace_uri.clone());
        removed_head.lineage = Some(workspace_uri.clone());
        enqueue_real_head(
            &mock,
            &workspace_uri,
            &removed_head_uri,
            "bafyremoved",
            &removed_head,
            &prior,
        );

        let mut opake = opake_for_identity(ALICE_DID, alice, mock.clone());
        let err = opake
            .repair_workspace_member_wrap(&workspace_id, &group_key, carol_did, None)
            .await
            .unwrap_err();
        assert!(matches!(err, crate::error::Error::CasConflict(_)));
        assert!(
            !mock
                .requests()
                .iter()
                .any(|request| request.url.contains("createRecord")),
            "stale repair must not overwrite a removal or re-add its member"
        );
    }

    // spec:key-rotation § The rotation event is synchronous and self-sufficient
    #[tokio::test]
    async fn removal_excludes_unverifiable_member_keeps_history_and_withholds_new_key() {
        let alice = Identity::generate(ALICE_DID, &mut OsRng);
        let bob = Identity::generate(BOB_DID, &mut OsRng);
        let carol_did = "did:plc:carol";
        let carol = Identity::generate(carol_did, &mut OsRng);
        let (workspace_uri, group_key, mut head) = real_genesis_keyring(
            ALICE_DID,
            vec![
                (ALICE_DID, Role::Manager, &alice),
                (BOB_DID, Role::Editor, &bob),
                (carol_did, Role::Editor, &carol),
            ],
        );
        let approval = approval_for_unverified_member(&workspace_uri, carol_did, &carol);
        head.members
            .iter_mut()
            .find(|member| member.did() == carol_did)
            .unwrap()
            .unverified_key_approval = Some(AtBytes::from_raw(&approval));
        let workspace_id = WorkspaceId::from_resolved(workspace_uri.clone());
        let mock = MockTransport::new();
        enqueue_real_head(
            &mock,
            &workspace_uri,
            &workspace_uri,
            "bafyhead",
            &head,
            &head,
        );
        // Carol's published record is unsigned even though her DID document
        // requires it.  This is a resolution error, so removal must continue
        // without a new Carol wrap rather than let her host veto Bob's removal.
        let anchor = crate::crypto::Ed25519SigningKey::from_bytes(&[72; 32])
            .verifying_key()
            .to_bytes();
        mock.enqueue(anchored_did_doc_response(
            carol_did,
            "https://pds.carol",
            &anchor,
        ));
        mock.enqueue(public_key_response(
            &format!("at://{carol_did}/at.opake.publicKey/self"),
            &carol,
        ));
        mock.enqueue(create_record_response(
            &format!("at://{ALICE_DID}/at.opake.keyring/3removal"),
            "bafyremoval",
        ));

        let mut opake = opake_for_identity(ALICE_DID, alice, mock.clone());
        let outcome = opake
            .remove_workspace_member(&workspace_id, &group_key, BOB_DID)
            .await
            .unwrap();
        assert_eq!(outcome.rotation, head.rotation + 1);
        assert_eq!(outcome.excluded_members.len(), 1);
        assert_eq!(outcome.excluded_members[0].did, carol_did);
        assert!(matches!(
            outcome.excluded_members[0].reason,
            ExcludedMemberReason::VerificationFailed
        ));

        let written = written_keyring(&mock);
        assert!(
            written.members.iter().all(|member| member.did() != BOB_DID),
            "removed member must not remain in the current head"
        );
        let carol_current = written
            .members
            .iter()
            .find(|member| member.did() == carol_did)
            .expect("excluded member remains admitted by DID and role");
        assert!(matches!(carol_current.role, Role::Editor));
        assert!(carol_current.wrapped_key.is_none());
        assert_eq!(
            carol_current
                .unverified_key_approval
                .as_ref()
                .unwrap()
                .decode()
                .unwrap(),
            approval,
            "an exclusion preserves the relationship's prior approval"
        );
        let historical_carol = written
            .key_history
            .iter()
            .find(|entry| entry.rotation == head.rotation)
            .and_then(|entry| {
                entry
                    .members
                    .iter()
                    .find(|member| member.did() == carol_did)
            })
            .expect("Carol's old real wrap remains in historical membership");
        assert!(historical_carol.wrapped_key.is_some());
        assert!(
            written
                .key_history
                .iter()
                .all(|entry| entry.members.iter().all(|member| member.did() != BOB_DID)),
            "removed member must not receive a historical snapshot in the new head"
        );
        assert!(
            Opake::<MockTransport, OsRng, NoopStorage>::try_unwrap_workspace_key(
                &written.members,
                carol_did,
                &workspace_uri,
                &carol.owned_private_keys().unwrap().bundle(),
                written.opake_version,
            )
            .unwrap()
            .is_none(),
            "a retained DID never receives an old wrap copied as the new generation"
        );
        assert!(matches!(
            Opake::<MockTransport, OsRng, NoopStorage>::try_unwrap_workspace_key(
                &written.members,
                BOB_DID,
                &workspace_uri,
                &bob.owned_private_keys().unwrap().bundle(),
                written.opake_version,
            ),
            Err(crate::error::Error::NotFound(_))
        ));
    }

    // spec:key-rotation § The rotation event is synchronous and self-sufficient
    #[tokio::test]
    async fn removal_rewraps_only_the_unchanged_approved_unverified_bundle() {
        let carol_did = "did:plc:carol";
        for bundle in ["unchanged", "x25519-changed", "ml-kem-changed"] {
            let alice = Identity::generate(ALICE_DID, &mut OsRng);
            let bob = Identity::generate(BOB_DID, &mut OsRng);
            let carol = Identity::generate(carol_did, &mut OsRng);
            let replacement = Identity::generate(carol_did, &mut OsRng);
            let (workspace_uri, group_key, mut head) = real_genesis_keyring(
                ALICE_DID,
                vec![
                    (ALICE_DID, Role::Manager, &alice),
                    (BOB_DID, Role::Editor, &bob),
                    (carol_did, Role::Editor, &carol),
                ],
            );
            let approval = approval_for_unverified_member(&workspace_uri, carol_did, &carol);
            head.members
                .iter_mut()
                .find(|member| member.did() == carol_did)
                .unwrap()
                .unverified_key_approval = Some(AtBytes::from_raw(&approval));
            let workspace_id = WorkspaceId::from_resolved(workspace_uri.clone());
            let mock = MockTransport::new();
            enqueue_real_head(
                &mock,
                &workspace_uri,
                &workspace_uri,
                "bafyhead",
                &head,
                &head,
            );
            mock.enqueue(did_doc_response(carol_did, "https://pds.carol"));
            let x25519 = carol.x25519_public_key_bytes().unwrap();
            let ml_kem = carol.ml_kem_public_key_bytes().unwrap();
            let replacement_x25519 = replacement.x25519_public_key_bytes().unwrap();
            let replacement_ml_kem = replacement.ml_kem_public_key_bytes().unwrap();
            mock.enqueue(public_key_response_for_bundle(
                &format!("at://{carol_did}/at.opake.publicKey/self"),
                if bundle == "x25519-changed" {
                    &replacement_x25519
                } else {
                    &x25519
                },
                if bundle == "ml-kem-changed" {
                    &replacement_ml_kem
                } else {
                    &ml_kem
                },
            ));
            mock.enqueue(create_record_response(
                &format!("at://{ALICE_DID}/at.opake.keyring/3{bundle}"),
                "bafyrotation",
            ));

            let mut opake = opake_for_identity(ALICE_DID, alice, mock.clone());
            let outcome = opake
                .remove_workspace_member(&workspace_id, &group_key, BOB_DID)
                .await
                .unwrap();
            let written = written_keyring(&mock);
            let carol_current = written
                .members
                .iter()
                .find(|member| member.did() == carol_did)
                .unwrap();
            if bundle == "unchanged" {
                assert!(outcome.excluded_members.is_empty());
                let private_keys = carol.owned_private_keys().unwrap();
                let unwrapped = crate::crypto::unwrap_key(
                    carol_current.wrapped_key.as_ref().unwrap(),
                    &private_keys.bundle(),
                    &crate::crypto::WrapContext::Keyring {
                        uri: &workspace_uri,
                    },
                    written.opake_version,
                )
                .unwrap();
                assert_eq!(unwrapped.0, outcome.group_key.0);
                assert_eq!(
                    carol_current
                        .unverified_key_approval
                        .as_ref()
                        .unwrap()
                        .decode()
                        .unwrap(),
                    approval
                );
            } else {
                assert!(carol_current.wrapped_key.is_none());
                assert!(matches!(
                    outcome.excluded_members.as_slice(),
                    [ExcludedMember {
                        did,
                        reason: ExcludedMemberReason::ApprovalRequired,
                    }] if did == carol_did
                ));
            }
        }
    }

    // spec:workspace-membership § Removal rotates the group key; leave does not
    #[tokio::test]
    async fn removal_self_wrap_uses_local_identity_without_counterparty_approval() {
        use crate::crypto::{PrivateKeyBundle, WrapContext};

        let alice = Identity::generate(ALICE_DID, &mut OsRng);
        let bob = Identity::generate(BOB_DID, &mut OsRng);
        let (workspace_uri, group_key, genesis) = real_genesis_keyring(
            ALICE_DID,
            vec![
                (ALICE_DID, Role::Manager, &alice),
                (BOB_DID, Role::Editor, &bob),
            ],
        );
        let workspace_id = WorkspaceId::from_resolved(workspace_uri.clone());
        let mock = MockTransport::new();
        enqueue_real_head(
            &mock,
            &workspace_uri,
            &workspace_uri,
            "bafygenesis",
            &genesis,
            &genesis,
        );
        mock.enqueue(create_record_response(
            &format!("at://{ALICE_DID}/at.opake.keyring/3selfwrap"),
            "bafyselfwrap",
        ));

        let mut opake = opake_for_identity(ALICE_DID, alice, mock.clone());
        let outcome = opake
            .remove_workspace_member(&workspace_id, &group_key, BOB_DID)
            .await
            .unwrap();
        let written = written_keyring(&mock);
        let alice_entry = written
            .members
            .iter()
            .find(|member| member.did() == ALICE_DID)
            .unwrap();
        assert!(alice_entry.unverified_key_approval.is_none());
        let x25519 = opake.identity().x25519_private_key_bytes().unwrap();
        let ml_kem = opake.identity().ml_kem_private_key_bytes().unwrap();
        let unwrapped = crate::crypto::unwrap_key(
            alice_entry.wrapped_key.as_ref().unwrap(),
            &PrivateKeyBundle {
                x25519: &x25519,
                ml_kem: &ml_kem,
            },
            &WrapContext::Keyring {
                uri: &workspace_uri,
            },
            written.opake_version,
        )
        .unwrap();
        assert_eq!(unwrapped.0, outcome.group_key.0);
        assert!(
            mock.requests()
                .iter()
                .all(|request| !request.url.contains("at.opake.publicKey")),
            "the author self-wrap must use local keys and make no approval-resolution request"
        );
    }

    // spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    #[tokio::test]
    async fn editor_self_removal_carries_remaining_wrap_presence_and_approval_verbatim() {
        let alice = Identity::generate(ALICE_DID, &mut OsRng);
        let bob = Identity::generate(BOB_DID, &mut OsRng);
        let carol_did = "did:plc:carol";
        let carol = Identity::generate(carol_did, &mut OsRng);
        let (workspace_uri, _, mut head) = real_genesis_keyring(
            ALICE_DID,
            vec![
                (ALICE_DID, Role::Manager, &alice),
                (BOB_DID, Role::Editor, &bob),
                (carol_did, Role::Editor, &carol),
            ],
        );
        let carol_before = head
            .members
            .iter_mut()
            .find(|member| member.did() == carol_did)
            .unwrap();
        carol_before.wrapped_key = None;
        let approval = approval_for_unverified_member(&workspace_uri, carol_did, &carol);
        carol_before.unverified_key_approval = Some(AtBytes::from_raw(&approval));
        let workspace_id = WorkspaceId::from_resolved(workspace_uri.clone());
        let mock = MockTransport::new();
        enqueue_real_head(
            &mock,
            &workspace_uri,
            &workspace_uri,
            "bafyhead",
            &head,
            &head,
        );
        mock.enqueue(create_record_response(
            &format!("at://{BOB_DID}/at.opake.keyring/3leave"),
            "bafyleave",
        ));

        let mut opake = opake_for_identity(BOB_DID, bob, mock.clone());
        opake.leave_workspace(&workspace_id).await.unwrap();
        let written = written_keyring(&mock);
        assert!(written.members.iter().all(|member| member.did() != BOB_DID));
        let carol_after = written
            .members
            .iter()
            .find(|member| member.did() == carol_did)
            .unwrap();
        assert!(carol_after.wrapped_key.is_none());
        assert_eq!(
            carol_after
                .unverified_key_approval
                .as_ref()
                .unwrap()
                .decode()
                .unwrap(),
            approval
        );
        assert_eq!(written.rotation, head.rotation);
        assert_eq!(
            serde_json::to_value(&written.key_history).unwrap(),
            serde_json::to_value(&head.key_history).unwrap()
        );
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
                members: vec![KeyringMember::with_wrap(wrapped, Role::Manager)],
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

    /// A verification-driven exclusion can remove a member's current wrap
    /// while retaining their rotation-zero wrap in `keyHistory`. A download of
    /// pre-exclusion content must return that historical key for the CLI cache;
    /// returning `ws.current_key()` would reject a successful historical read.
    // spec:workspace-membership § Removal rotates the group key; leave does not
    #[tokio::test]
    #[allow(non_snake_case)] // bug__ regression-naming convention
    async fn bug__historical_member_download_returns_document_rotation_key() {
        use crate::crypto::{
            encrypt_blob, encrypt_metadata, generate_content_key, wrap_content_key_for_keyring,
            DocumentMetadata, SealContext, SealType,
        };
        use crate::records::{
            BlobRef, CidLink, Document, Encryption, KeyringEncryption, KeyringRef,
        };
        use base64::Engine;

        const DOC_URI: &str = "at://did:plc:alice/at.opake.document/historical";
        const HEAD_URI: &str = "at://did:plc:alice/at.opake.keyring/current-head";

        let mock = MockTransport::new();
        let member = Identity::generate(BOB_DID, &mut OsRng);
        let owner = Identity::generate(ALICE_DID, &mut OsRng);
        let mut opake = opake_for_identity(BOB_DID, member, mock.clone());
        let member_identity = opake.identity();

        let (workspace_id, historical_group_key, genesis) = real_genesis_keyring(
            ALICE_DID,
            vec![
                (ALICE_DID, Role::Manager, &owner),
                (BOB_DID, Role::Editor, member_identity),
            ],
        );
        let current_group_key = generate_content_key(&mut OsRng);
        let mut head = real_rotated_head(
            &workspace_id,
            &genesis,
            &current_group_key,
            vec![(ALICE_DID, Role::Manager, &owner)],
        );
        head.members.push(KeyringMember {
            did: BOB_DID.into(),
            role: Role::Editor,
            wrapped_key: None,
            unverified_key_approval: Some(AtBytes {
                encoded: base64::engine::general_purpose::STANDARD.encode([0u8; 32]),
            }),
        });
        assert!(head
            .members
            .iter()
            .find(|member| member.did() == BOB_DID)
            .is_some_and(|member| member.wrapped_key.is_none()));

        let content_key = generate_content_key(&mut OsRng);
        let blob = encrypt_blob(
            &content_key,
            b"historical workspace content",
            &SealContext::new(DOC_URI, SealType::DocumentBlob),
            &mut OsRng,
        )
        .unwrap();
        let metadata = encrypt_metadata(
            &content_key,
            &DocumentMetadata {
                name: "historical.txt".into(),
                mime_type: Some("text/plain".into()),
                size: Some(28),
                tags: vec![],
                description: None,
            },
            &SealContext::new(DOC_URI, SealType::DocumentMetadata),
            &mut OsRng,
        )
        .unwrap();
        let wrapped_content_key =
            wrap_content_key_for_keyring(&content_key, &historical_group_key).unwrap();
        let document = Document::new(
            BlobRef {
                blob_type: "blob".into(),
                reference: CidLink {
                    cid: "bafyhistoricalblob".into(),
                },
                mime_type: "application/octet-stream".into(),
                size: blob.ciphertext.len() as u64,
            },
            Encryption::Keyring(KeyringEncryption {
                keyring_ref: KeyringRef {
                    keyring: workspace_id.clone(),
                    wrapped_content_key: AtBytes {
                        encoded: base64::engine::general_purpose::STANDARD
                            .encode(wrapped_content_key),
                    },
                    rotation: 0,
                },
                algo: "aes-256-gcm".into(),
                nonce: AtBytes {
                    encoded: base64::engine::general_purpose::STANDARD.encode(blob.nonce),
                },
            }),
            metadata,
            "2026-09-12T00:00:02Z".into(),
        )
        .with_workspace_id(workspace_id.clone());
        let document_response = || HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "uri": DOC_URI,
                "cid": "bafydoc",
                "value": document,
            }))
            .unwrap(),
        };

        // document reference, indexer head + keyring chain, workspace
        // resolution, then the actual document/blob download.
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.did-plc-alice"));
        mock.enqueue(document_response());
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&serde_json::json!({
                "workspace_id": workspace_id,
                "keyring": { "head_uri": HEAD_URI, "head_cid": "bafyhead" },
                "root_directory": null,
            }))
            .unwrap(),
        });
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.did-plc-alice"));
        mock.enqueue(get_keyring_response(HEAD_URI, "bafyhead", &head));
        mock.enqueue(get_keyring_response(&workspace_id, "bafygenesis", &genesis));
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.did-plc-alice"));
        mock.enqueue(get_keyring_response(HEAD_URI, "bafyhead", &head));
        mock.enqueue(did_doc_response(ALICE_DID, "https://pds.did-plc-alice"));
        mock.enqueue(document_response());
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: blob.ciphertext.clone(),
        });

        let result = opake.download_as_workspace_member(DOC_URI).await.unwrap();
        assert_eq!(result.plaintext, b"historical workspace content");
        assert_eq!(result.rotation, 0);
        assert_eq!(result.group_key.0, historical_group_key.0);
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
            members: vec![KeyringMember::with_wrap(wrapped, Role::Manager)],
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
