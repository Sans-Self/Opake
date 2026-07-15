//! End-to-end tests for the directory-additivity slow path.
//!
//! `verify_directory_additivity` (the pure check in `directories::chain`)
//! is unit-tested there with static manager predicates. What it *can't*
//! express is time-varying authority: a DID that was a manager when it
//! authored a deletion but isn't one now. That case only surfaces through
//! `FileManager::verify_directory_chain_additivity`, which walks the
//! keyring chain to recover historical authority. These tests drive that
//! method end-to-end against a mocked keyring chain.

use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
use crate::crypto::{generate_content_key, OsRng};
use crate::error::Error;
use crate::manager::types::FileContext;
use crate::opake::Opake;
use crate::records::{
    AtBytes, CidLink, DirectKeyWrapping, Directory, KeyWrapping, Keyring, KeyringMember,
    ListingEntry, Role, WrappedKey, SCHEMA_VERSION,
};
use crate::storage::{CachedRecord, Identity, NoopStorage};
use crate::test_utils::{dummy_encrypted_metadata, MockTransport};
use crate::workspace::Workspace;

const ALICE: &str = "did:plc:alice";
const BOB: &str = "did:plc:bob";
const PDS_A: &str = "https://pds.alice.example.com";
const PDS_B: &str = "https://pds.bob.example.com";

// Keyring chain: bob's head supersedes alice's genesis. Alice is a
// manager only in the genesis — a *former* manager from the head's view.
const KEYRING_HEAD: &str = "at://did:plc:bob/at.opake.keyring/head";
const KEYRING_GENESIS: &str = "at://did:plc:alice/at.opake.keyring/genesis";

const DIR_GENESIS: &str = "at://did:plc:bob/at.opake.directory/dgen";
const DIR_DEL_BY_ALICE: &str = "at://did:plc:alice/at.opake.directory/ddel";
const DIR_DEL_BY_CHARLIE: &str = "at://did:plc:charlie/at.opake.directory/ddel";
const DOC1: &str = "at://did:plc:bob/at.opake.document/doc1";
const DOC2: &str = "at://did:plc:bob/at.opake.document/doc2";

fn ok(body: serde_json::Value) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&body).unwrap(),
    }
}

fn not_found() -> HttpResponse {
    HttpResponse {
        status: 404,
        headers: vec![],
        body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
    }
}

fn did_doc(did: &str, pds_url: &str) -> serde_json::Value {
    serde_json::json!({
        "id": did,
        "alsoKnownAs": [],
        "service": [{
            "id": "#atproto_pds",
            "type": "AtprotoPersonalDataServer",
            "serviceEndpoint": pds_url,
        }]
    })
}

fn record_entry(uri: &str, cid: &str, value: &impl serde::Serialize) -> serde_json::Value {
    serde_json::json!({ "uri": uri, "cid": cid, "value": value })
}

fn member(did: &str, role: Role) -> KeyringMember {
    KeyringMember {
        wrapped_key: WrappedKey {
            did: did.into(),
            ciphertext: AtBytes {
                encoded: "AAAA".into(),
            },
            algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
        },
        role,
    }
}

fn keyring(members: Vec<KeyringMember>, supersedes: Option<&str>) -> Keyring {
    Keyring {
        opake_version: SCHEMA_VERSION,
        algo: "aes-256-gcm".into(),
        members,
        rotation: 0,
        key_history: Vec::new(),
        encrypted_metadata: dummy_encrypted_metadata(),
        supersedes: supersedes.map(String::from),
        workspace_id: supersedes.map(|_| KEYRING_GENESIS.to_string()),
        created_at: "2026-03-01T00:00:00Z".into(),
        modified_at: None,
    }
}

fn dir(entries: Vec<&str>, supersedes: Option<&str>) -> Directory {
    Directory {
        opake_version: SCHEMA_VERSION,
        key_wrapping: KeyWrapping::Direct(DirectKeyWrapping { keys: vec![] }),
        encrypted_metadata: dummy_encrypted_metadata(),
        entries: entries
            .into_iter()
            .map(|target| ListingEntry {
                target: target.into(),
                target_cid: CidLink {
                    cid: "bafyfake".into(),
                },
            })
            .collect(),
        supersedes: supersedes.map(String::from),
        workspace_id: None,
        is_workspace_root: false,
        created_at: "2026-03-01T00:00:00Z".into(),
        modified_at: None,
    }
}

fn cached(uri: &str, directory: &Directory) -> CachedRecord {
    CachedRecord {
        uri: uri.into(),
        cid: "bafydir".into(),
        value: serde_json::to_value(directory).unwrap(),
    }
}

/// Build an Opake whose transport is `mock`, acting as `BOB`.
fn opake_for_bob(mock: MockTransport) -> Opake<MockTransport, OsRng, NoopStorage> {
    let session = Session::Legacy(LegacySession {
        did: BOB.into(),
        handle: "bob.handle".into(),
        access_jwt: "jwt".into(),
        refresh_jwt: "refresh".into(),
    });
    let client = XrpcClient::with_session(mock, "https://pds.test".into(), session);
    let identity = Identity::generate(BOB, &mut OsRng);
    Opake::new(client, BOB.into(), identity, OsRng, NoopStorage, || {
        1_700_000_000_000_000
    })
    .unwrap()
}

/// A workspace whose *current* manager is bob alone — alice has been
/// removed since she authored her deletion.
fn workspace_bob_only_manager() -> Workspace {
    Workspace::from_keyring(
        KEYRING_HEAD.into(),
        "Test WS".into(),
        None,
        BOB.into(),
        generate_content_key(&mut OsRng),
        1,
        Vec::new(),
        vec![BOB.into()],
    )
}

/// Enqueue the four responses a `walk_back_to_genesis` over the keyring
/// chain consumes: DID resolution + getRecord for the head (bob), then the
/// same for the genesis (alice). Genesis lists both alice and bob as
/// managers, so the "ever a manager" union is {alice, bob}.
fn enqueue_keyring_chain(mock: &MockTransport) {
    mock.enqueue(ok(did_doc(BOB, PDS_B)));
    mock.enqueue(ok(record_entry(
        KEYRING_HEAD,
        "bafyhead",
        &keyring(vec![member(BOB, Role::Manager)], Some(KEYRING_GENESIS)),
    )));
    mock.enqueue(ok(did_doc(ALICE, PDS_A)));
    mock.enqueue(ok(record_entry(
        KEYRING_GENESIS,
        "bafygen",
        &keyring(
            vec![member(ALICE, Role::Manager), member(BOB, Role::Manager)],
            None,
        ),
    )));
}

/// A former manager's deletion must survive the recheck: alice deleted
/// doc2 while she was a manager, then was removed. Her deletion stays in
/// the directory chain forever. The fast path (current managers = [bob])
/// flags it as non-additive; the slow path walks the keyring chain, finds
/// alice was *ever* a manager, and clears it. A current-managers-only
/// check would brick `load_tree` for everyone here — that's the
/// regression this guards.
#[tokio::test]
async fn additivity_allows_former_manager_deletion() {
    let mock = MockTransport::new();
    enqueue_keyring_chain(&mock);

    let mut opake = opake_for_bob(mock.clone());
    let ctx = FileContext::Workspace(workspace_bob_only_manager());
    let mgr = opake.file_manager(&ctx);

    let records = vec![
        cached(DIR_GENESIS, &dir(vec![DOC1, DOC2], None)),
        cached(DIR_DEL_BY_ALICE, &dir(vec![DOC1], Some(DIR_GENESIS))),
    ];

    mgr.verify_directory_chain_additivity(&records, &[])
        .await
        .expect("former manager's deletion must pass after the keyring-chain recheck");

    // The slow path must actually have walked the chain — four calls.
    assert_eq!(
        mock.requests().len(),
        4,
        "expected the keyring chain walk (2 DID resolutions + 2 getRecord)"
    );
}

/// The slow path must still *reject* a genuine editor non-additive
/// supersede. Charlie was never a manager in any keyring version, so the
/// "ever a manager" union doesn't exempt him — the violation stands.
#[tokio::test]
async fn additivity_rejects_never_manager_deletion() {
    let mock = MockTransport::new();
    enqueue_keyring_chain(&mock);

    let mut opake = opake_for_bob(mock.clone());
    let ctx = FileContext::Workspace(workspace_bob_only_manager());
    let mgr = opake.file_manager(&ctx);

    let records = vec![
        cached(DIR_GENESIS, &dir(vec![DOC1, DOC2], None)),
        cached(DIR_DEL_BY_CHARLIE, &dir(vec![DOC1], Some(DIR_GENESIS))),
    ];

    let err = mgr
        .verify_directory_chain_additivity(&records, &[])
        .await
        .expect_err("a never-manager's deletion must still be rejected");

    match err {
        Error::ChainAdditivityViolation {
            uri,
            author_did,
            missing,
        } => {
            assert_eq!(uri, DIR_DEL_BY_CHARLIE);
            assert_eq!(author_did, "did:plc:charlie");
            assert_eq!(missing, vec![DOC2.to_string()]);
        }
        other => panic!("expected ChainAdditivityViolation, got: {other:?}"),
    }
}

/// Defense-in-depth, not a gate: if the keyring chain can't be walked
/// (offline, indexer/PDS down) we fail **open** rather than brick an
/// offline tree load. The indexer enforces additivity at write time, so a
/// recheck we can't complete shouldn't be fatal. Here the head's DID
/// resolution 404s, so the walk errors and the violation is waved through.
#[tokio::test]
async fn additivity_fails_open_when_keyring_chain_unreachable() {
    let mock = MockTransport::new();
    // First call in the walk is the head DID resolution — make it 404.
    mock.enqueue(not_found());

    let mut opake = opake_for_bob(mock.clone());
    let ctx = FileContext::Workspace(workspace_bob_only_manager());
    let mgr = opake.file_manager(&ctx);

    let records = vec![
        cached(DIR_GENESIS, &dir(vec![DOC1, DOC2], None)),
        cached(DIR_DEL_BY_ALICE, &dir(vec![DOC1], Some(DIR_GENESIS))),
    ];

    mgr.verify_directory_chain_additivity(&records, &[])
        .await
        .expect("fail-open: an unreachable keyring chain must not brick the load");
}

/// Regression: an editor's cross-author doc edit drops the original doc
/// entry and adds one that *supersedes* it. The supersede-aware rule must
/// clear it — but the coverage link lives on the new *document*, which is
/// cached under a different scope than the directory records, so it has to
/// be supplied separately. Without the document records the edit reads as a
/// bare delete and trips a false `ChainAdditivityViolation` (the symptom: a
/// successfully-saved edit shows up as a violation in the file list).
#[tokio::test]
#[allow(non_snake_case)] // bug__ regression-naming convention
async fn bug__additivity_allows_editor_doc_edit_via_document_supersede() {
    const F1: &str = "at://did:plc:alice/at.opake.document/f1";
    const F2: &str = "at://did:plc:charlie/at.opake.document/f2";
    const DIR_EDIT_BY_CHARLIE: &str = "at://did:plc:charlie/at.opake.directory/dedit";

    // Fast path: F1 is dropped but covered by F2's supersede, so the check
    // passes without walking the keyring chain — no mocked responses needed.
    let mock = MockTransport::new();
    let mut opake = opake_for_bob(mock.clone());
    let ctx = FileContext::Workspace(workspace_bob_only_manager());
    let mgr = opake.file_manager(&ctx);

    let records = vec![
        cached(DIR_GENESIS, &dir(vec![DOC1, F1], None)),
        cached(DIR_EDIT_BY_CHARLIE, &dir(vec![DOC1, F2], Some(DIR_GENESIS))),
    ];
    let docs = vec![CachedRecord {
        uri: F2.into(),
        cid: String::new(),
        value: serde_json::json!({ "supersedes": F1 }),
    }];

    mgr.verify_directory_chain_additivity(&records, &docs)
        .await
        .expect("editor doc edit via document supersede must pass additivity");
}

/// The negative: same shape, but the added document supersedes nothing — a
/// genuine drop. Even with documents supplied, this must still be rejected
/// (charlie was never a manager, so the keyring-chain recheck doesn't
/// exempt him either).
#[tokio::test]
#[allow(non_snake_case)] // bug__ regression-naming convention
async fn bug__additivity_rejects_editor_drop_without_document_supersede() {
    const F1: &str = "at://did:plc:alice/at.opake.document/f1";
    const F2: &str = "at://did:plc:charlie/at.opake.document/f2";
    const DIR_EDIT_BY_CHARLIE: &str = "at://did:plc:charlie/at.opake.directory/dedit";

    let mock = MockTransport::new();
    enqueue_keyring_chain(&mock);
    let mut opake = opake_for_bob(mock.clone());
    let ctx = FileContext::Workspace(workspace_bob_only_manager());
    let mgr = opake.file_manager(&ctx);

    let records = vec![
        cached(DIR_GENESIS, &dir(vec![DOC1, F1], None)),
        cached(DIR_EDIT_BY_CHARLIE, &dir(vec![DOC1, F2], Some(DIR_GENESIS))),
    ];
    let docs = vec![CachedRecord {
        uri: F2.into(),
        cid: String::new(),
        value: serde_json::json!({}),
    }];

    let err = mgr
        .verify_directory_chain_additivity(&records, &docs)
        .await
        .expect_err("a drop with no superseding document must be rejected");
    assert!(matches!(err, Error::ChainAdditivityViolation { .. }));
}
