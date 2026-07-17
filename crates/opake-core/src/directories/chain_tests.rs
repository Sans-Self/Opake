use super::*;
use crate::client::HttpResponse;
use crate::records::Directory;
use crate::test_utils::MockTransport;

use super::super::tests::{dummy_directory, dummy_directory_with_entries};

const DID_A: &str = "did:plc:alice";
const DID_B: &str = "did:plc:bob";
const PDS_A: &str = "https://pds.alice.example.com";
const PDS_B: &str = "https://pds.bob.example.com";

const URI_GENESIS: &str = "at://did:plc:alice/at.opake.directory/genesis";
const URI_MIDDLE: &str = "at://did:plc:bob/at.opake.directory/middle";
const URI_HEAD: &str = "at://did:plc:alice/at.opake.directory/head";

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

fn record_entry(uri: &str, cid: &str, dir: &Directory) -> serde_json::Value {
    serde_json::json!({
        "uri": uri,
        "cid": cid,
        "value": dir,
    })
}

/// Build a directory that supersedes another URI.
fn dir_superseding(name: &str, prior_uri: &str) -> Directory {
    let mut dir = dummy_directory(name);
    dir.supersedes = Some(prior_uri.to_owned());
    dir
}

#[tokio::test]
async fn fetch_chain_node_resolves_pds_and_returns_record() {
    let mock = MockTransport::new();
    let dir = dummy_directory_with_entries("/", vec!["at://did:plc:x/at.opake.document/d1".into()]);

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_GENESIS, "bafygenesis", &dir)));

    let node: ChainNode<Directory> = fetch_chain_node(&mock, URI_GENESIS).await.unwrap();

    assert_eq!(node.uri, URI_GENESIS);
    assert_eq!(node.cid, "bafygenesis");
    assert_eq!(node.record.entries.len(), 1);

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2);
    assert!(
        reqs[0].url.contains(DID_A),
        "first request hits PLC for DID resolution"
    );
    assert!(
        reqs[1].url.starts_with(PDS_A),
        "second request goes to the resolved PDS, got {}",
        reqs[1].url
    );
    assert!(reqs[1].url.contains("getRecord"));
}

#[tokio::test]
async fn fetch_chain_node_rejects_malformed_uri() {
    let mock = MockTransport::new();
    let err = fetch_chain_node::<Directory>(&mock, "not-an-at-uri")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidRecord(_)));
    assert!(
        mock.requests().is_empty(),
        "no network call on malformed URI"
    );
}

#[tokio::test]
async fn fetch_chain_node_propagates_not_found() {
    let mock = MockTransport::new();
    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(not_found());

    let err = fetch_chain_node::<Directory>(&mock, URI_GENESIS)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

/// A `record_entry` whose `value` is a raw JSON object — for building links
/// the typed constructors can't express (missing `opakeVersion`, a future
/// version, structurally malformed).
fn raw_record_entry(uri: &str, cid: &str, value: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "uri": uri, "cid": cid, "value": value })
}

// record-validity § writes refuse state they do not fully understand
// tree-chains § unverifiable heads degrade to the last verifiable state
#[tokio::test]
#[allow(non_snake_case)] // bug__ regression-naming convention
async fn bug__corrupt_chain_link_rejects_head_naming_the_link() {
    // A chain link whose bytes don't parse (here: missing the required
    // `keyWrapping`/`encryptedMetadata`, and no `opakeVersion`) must fail the
    // walk with `ChainLinkCorrupt` naming the link — never a silent adoption.
    let mock = MockTransport::new();
    let malformed = serde_json::json!({ "createdAt": "2026-03-01T00:00:00Z" });

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(raw_record_entry(URI_HEAD, "bafyhead", malformed)));

    let err = walk_back_to_genesis::<Directory>(&mock, URI_HEAD)
        .await
        .unwrap_err();

    match err {
        Error::ChainLinkCorrupt { uri } => assert_eq!(uri, URI_HEAD),
        other => panic!("expected ChainLinkCorrupt, got {other:?}"),
    }
}

// record-validity § future-version records are visible, locked, and actionable
#[tokio::test]
async fn future_version_chain_link_requires_newer_client() {
    // A well-formed link declaring a version newer than this client supports
    // must halt the walk with an actionable "newer client required" error that
    // names the link — writes against it are refused, the block is self-explanatory.
    let mock = MockTransport::new();
    let mut value = serde_json::to_value(dummy_directory("/")).unwrap();
    value["opakeVersion"] = serde_json::json!(crate::records::SCHEMA_VERSION + 1);

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(raw_record_entry(URI_HEAD, "bafyhead", value)));

    let err = walk_back_to_genesis::<Directory>(&mock, URI_HEAD)
        .await
        .unwrap_err();

    match &err {
        Error::ChainLinkNeedsNewerClient {
            uri,
            version,
            supported,
        } => {
            assert_eq!(uri, URI_HEAD);
            assert_eq!(*version, crate::records::SCHEMA_VERSION + 1);
            assert_eq!(*supported, crate::records::SCHEMA_VERSION);
        }
        other => panic!("expected ChainLinkNeedsNewerClient, got {other:?}"),
    }
    // Message is actionable: it tells the user to update.
    assert!(err.to_string().to_lowercase().contains("update"));
}

// record-validity § writes refuse state they do not fully understand
#[tokio::test]
async fn corrupt_intermediate_link_rejects_the_whole_chain() {
    // The head parses, but an intermediate link it supersedes is corrupt. The
    // walk crosses the corrupt link and refuses — the proposed head is never
    // adopted (integrity over liveness), and the corrupt intermediate is named.
    let mock = MockTransport::new();
    let head = dir_superseding("/", URI_MIDDLE);
    let malformed_middle = serde_json::json!({ "createdAt": "2026-03-01T00:00:00Z" });

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_HEAD, "bafyhead", &head)));
    mock.enqueue(ok(did_doc(DID_B, PDS_B)));
    mock.enqueue(ok(raw_record_entry(
        URI_MIDDLE,
        "bafymiddle",
        malformed_middle,
    )));

    let err = walk_back_to_genesis::<Directory>(&mock, URI_HEAD)
        .await
        .unwrap_err();

    match err {
        Error::ChainLinkCorrupt { uri } => assert_eq!(uri, URI_MIDDLE),
        other => panic!("expected ChainLinkCorrupt at the intermediate, got {other:?}"),
    }
}

// -- walk_back_to_genesis --

// spec:tree-chains § A path's canonical state is the head of a supersede chain
#[tokio::test]
async fn walk_back_returns_genesis_only_when_no_supersedes() {
    let mock = MockTransport::new();
    let genesis = dummy_directory("/");

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_GENESIS, "bafygenesis", &genesis)));

    let chain: Vec<ChainNode<Directory>> = walk_back_to_genesis(&mock, URI_GENESIS).await.unwrap();

    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].uri, URI_GENESIS);
    assert!(chain[0].record.supersedes.is_none());
}

// spec:tree-chains § A path's canonical state is the head of a supersede chain
#[tokio::test]
async fn walk_back_traverses_chain_in_head_to_genesis_order() {
    let mock = MockTransport::new();
    let genesis = dummy_directory("/");
    let middle = dir_superseding("/", URI_GENESIS);
    let head = dir_superseding("/", URI_MIDDLE);

    // Walking head → middle → genesis. Three hops across two distinct
    // PDSes (DID_A twice, DID_B once). The walk caches DID→PDS, so we
    // expect 2 PLC resolutions + 3 record fetches = 5 calls.
    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_HEAD, "bafyhead", &head)));
    mock.enqueue(ok(did_doc(DID_B, PDS_B)));
    mock.enqueue(ok(record_entry(URI_MIDDLE, "bafymiddle", &middle)));
    mock.enqueue(ok(record_entry(URI_GENESIS, "bafygenesis", &genesis)));

    let chain: Vec<ChainNode<Directory>> = walk_back_to_genesis(&mock, URI_HEAD).await.unwrap();

    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0].uri, URI_HEAD);
    assert_eq!(chain[1].uri, URI_MIDDLE);
    assert_eq!(chain[2].uri, URI_GENESIS);

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 5);
    assert!(
        reqs[3].url.starts_with(PDS_B),
        "middle hop fetches from PDS_B, got {}",
        reqs[3].url
    );
    assert!(
        reqs[4].url.starts_with(PDS_A),
        "genesis hop reuses cached PDS_A, got {}",
        reqs[4].url
    );
}

/// A record whose declared lineage disagrees with its predecessor's anchor is
/// outside the chain. The back-walk stops at the flipped edge read-leniently:
/// it returns the flipped head alone rather than following its illegitimate
/// back-edge into the prior chain, so genesis-matching downstream rejects it.
// spec:lineage § Lineage never flips across a supersede
#[tokio::test]
async fn walk_back_stops_at_a_flipped_lineage() {
    let mock = MockTransport::new();

    // A well-formed genesis, and a head that supersedes it but declares a
    // lineage pointing at a foreign chain.
    let genesis = dummy_directory("/");
    let mut head = dir_superseding("/", URI_GENESIS);
    head.lineage = Some("at://did:plc:attacker/at.opake.directory/other".into());

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_HEAD, "bafyhead", &head)));
    // The genesis record is available, but the walk must not treat it as part
    // of this chain once the flip is detected.
    mock.enqueue(ok(record_entry(URI_GENESIS, "bafygenesis", &genesis)));

    let chain: Vec<ChainNode<Directory>> = walk_back_to_genesis(&mock, URI_HEAD).await.unwrap();

    assert_eq!(
        chain.len(),
        1,
        "flipped head is not linked to the prior chain"
    );
    assert_eq!(chain[0].uri, URI_HEAD);

    // And the integrity wrapper rejects the flipped head against the real
    // genesis rather than silently accepting a foreign chain.
    let mock2 = MockTransport::new();
    mock2.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock2.enqueue(ok(record_entry(URI_HEAD, "bafyhead", &head)));
    mock2.enqueue(ok(record_entry(URI_GENESIS, "bafygenesis", &genesis)));
    let err = verify_and_walk_chain::<Directory>(&mock2, URI_HEAD, URI_GENESIS)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::ChainGenesisMismatch { .. }));
}

// spec:tree-chains § A path's canonical state is the head of a supersede chain
#[tokio::test]
async fn walk_back_rejects_cycle() {
    // A → B → A. Walking back from A revisits A on the third hop and must
    // fail with ChainCycle rather than looping or hitting an arbitrary cap.
    let mock = MockTransport::new();
    let head = dir_superseding("/", URI_MIDDLE);
    let middle = dir_superseding("/", URI_HEAD);

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_HEAD, "bafyhead", &head)));
    mock.enqueue(ok(did_doc(DID_B, PDS_B)));
    mock.enqueue(ok(record_entry(URI_MIDDLE, "bafymiddle", &middle)));

    let err = walk_back_to_genesis::<Directory>(&mock, URI_HEAD)
        .await
        .unwrap_err();

    match err {
        Error::ChainCycle { uri } => assert_eq!(uri, URI_HEAD),
        other => panic!("expected ChainCycle, got: {other:?}"),
    }
}

// spec:tree-chains § A path's canonical state is the head of a supersede chain
#[tokio::test]
async fn walk_back_propagates_missing_intermediate() {
    let mock = MockTransport::new();
    let head = dir_superseding("/", URI_GENESIS);

    // First hop OK; second hop reuses cached DID_A→PDS_A then 404s on the
    // record fetch.
    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_HEAD, "bafyhead", &head)));
    mock.enqueue(not_found());

    let err = walk_back_to_genesis::<Directory>(&mock, URI_HEAD)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

// -- verify_and_walk_chain --
//
// The integrity wrapper around walk_back_to_genesis. Pins the chain
// tail to an expected genesis URI so an indexer that lies about which
// chain a head belongs to can't slip a foreign head past us.

// spec:tree-chains § A path's canonical state is the head of a supersede chain
#[tokio::test]
async fn verify_and_walk_chain_accepts_matching_genesis() {
    let mock = MockTransport::new();
    let genesis = dummy_directory("/");
    let head = dir_superseding("/", URI_GENESIS);

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_HEAD, "bafyhead", &head)));
    mock.enqueue(ok(record_entry(URI_GENESIS, "bafygenesis", &genesis)));

    let chain: Vec<ChainNode<Directory>> = verify_and_walk_chain(&mock, URI_HEAD, URI_GENESIS)
        .await
        .unwrap();

    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].uri, URI_HEAD);
    assert_eq!(chain[1].uri, URI_GENESIS);
}

// spec:tree-chains § A path's canonical state is the head of a supersede chain
#[tokio::test]
async fn verify_and_walk_chain_accepts_genesis_only_chain() {
    // Freshly-created workspace: head and genesis are the same record.
    let mock = MockTransport::new();
    let genesis = dummy_directory("/");

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_GENESIS, "bafygenesis", &genesis)));

    let chain: Vec<ChainNode<Directory>> = verify_and_walk_chain(&mock, URI_GENESIS, URI_GENESIS)
        .await
        .unwrap();

    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].uri, URI_GENESIS);
    assert!(chain[0].record.supersedes.is_none());
}

// spec:tree-chains § A path's canonical state is the head of a supersede chain
#[tokio::test]
async fn verify_and_walk_chain_rejects_wrong_genesis() {
    // The indexer's claimed head walks back to a *real* genesis, but
    // not the one the caller asked about. Classic "head from another
    // workspace" attack.
    let mock = MockTransport::new();
    let genesis = dummy_directory("/");
    let head = dir_superseding("/", URI_GENESIS);

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_HEAD, "bafyhead", &head)));
    mock.enqueue(ok(record_entry(URI_GENESIS, "bafygenesis", &genesis)));

    let other_workspace = "at://did:plc:other/at.opake.directory/elsewhere";
    let err = verify_and_walk_chain::<Directory>(&mock, URI_HEAD, other_workspace)
        .await
        .unwrap_err();

    match err {
        Error::ChainGenesisMismatch { expected, actual } => {
            assert_eq!(expected, other_workspace);
            assert_eq!(actual, URI_GENESIS);
        }
        other => panic!("expected ChainGenesisMismatch, got: {other:?}"),
    }
}

// spec:tree-chains § A path's canonical state is the head of a supersede chain
#[tokio::test]
async fn verify_and_walk_chain_propagates_broken_chain() {
    // Walking back fails partway through. The verification wrapper
    // should surface the underlying error rather than swallowing it.
    let mock = MockTransport::new();
    let head = dir_superseding("/", URI_GENESIS);

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_HEAD, "bafyhead", &head)));
    mock.enqueue(not_found());

    let err = verify_and_walk_chain::<Directory>(&mock, URI_HEAD, URI_GENESIS)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

// spec:tree-chains § A path's canonical state is the head of a supersede chain
#[tokio::test]
async fn verify_and_walk_chain_propagates_cycle() {
    // A → B → A cycle. ChainCycle should propagate through, not get
    // hidden behind a generic mismatch.
    let mock = MockTransport::new();
    let head = dir_superseding("/", URI_MIDDLE);
    let middle = dir_superseding("/", URI_HEAD);

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_HEAD, "bafyhead", &head)));
    mock.enqueue(ok(did_doc(DID_B, PDS_B)));
    mock.enqueue(ok(record_entry(URI_MIDDLE, "bafymiddle", &middle)));

    let err = verify_and_walk_chain::<Directory>(&mock, URI_HEAD, URI_GENESIS)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::ChainCycle { .. }));
}

// -- verify_keyring_chain_authority --
//
// Authorization trail check on a fetched keyring chain. Verifies every
// supersede was authored by a manager of the prior keyring. Pure
// function over a pre-fetched chain — no network calls.

mod keyring_authority {
    use super::*;
    use crate::records::{AtBytes, Keyring, KeyringMember, Role, WrappedKey, SCHEMA_VERSION};
    use crate::test_utils::dummy_encrypted_metadata;

    const KEYRING_GENESIS: &str = "at://did:plc:alice/at.opake.keyring/genesis";
    const KEYRING_HEAD: &str = "at://did:plc:alice/at.opake.keyring/head";
    const KEYRING_HEAD_BOB: &str = "at://did:plc:bob/at.opake.keyring/head";

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
            lineage: supersedes.map(|_| KEYRING_GENESIS.to_string()),
            created_at: "2026-03-01T00:00:00Z".into(),
            modified_at: None,
        }
    }

    fn node(uri: &str, record: Keyring) -> ChainNode<Keyring> {
        ChainNode {
            uri: uri.into(),
            cid: format!("bafy{uri}"),
            record,
        }
    }

    // spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    #[test]
    fn accepts_genesis_only_chain() {
        // A chain of length 1 is just the genesis. No supersedes means
        // nothing to verify; pass cleanly.
        let chain = vec![node(
            KEYRING_GENESIS,
            keyring(vec![member(DID_A, Role::Manager)], None),
        )];
        verify_keyring_chain_authority(&chain).unwrap();
    }

    // spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    #[test]
    fn accepts_manager_authored_supersede() {
        let chain = vec![
            // Head authored by Alice (a manager in the genesis below).
            node(
                KEYRING_HEAD,
                keyring(
                    vec![member(DID_A, Role::Manager), member(DID_B, Role::Editor)],
                    Some(KEYRING_GENESIS),
                ),
            ),
            node(
                KEYRING_GENESIS,
                keyring(vec![member(DID_A, Role::Manager)], None),
            ),
        ];
        verify_keyring_chain_authority(&chain).unwrap();
    }

    // spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    #[test]
    fn rejects_editor_authored_supersede() {
        // Head is at Bob's DID; Bob is only an Editor in the prior
        // keyring, not a Manager. Reject.
        let chain = vec![
            node(
                KEYRING_HEAD_BOB,
                keyring(
                    vec![member(DID_A, Role::Manager), member(DID_B, Role::Manager)],
                    Some(KEYRING_GENESIS),
                ),
            ),
            node(
                KEYRING_GENESIS,
                keyring(
                    vec![member(DID_A, Role::Manager), member(DID_B, Role::Editor)],
                    None,
                ),
            ),
        ];
        let err = verify_keyring_chain_authority(&chain).unwrap_err();
        match err {
            Error::ChainAuthorityViolation { uri, author_did } => {
                assert_eq!(uri, KEYRING_HEAD_BOB);
                assert_eq!(author_did, DID_B);
            }
            other => panic!("expected ChainAuthorityViolation, got: {other:?}"),
        }
    }

    // spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    #[test]
    fn rejects_non_member_authored_supersede() {
        // Head is at Bob's DID; Bob is not in the prior keyring at all.
        // Same rejection signal as the editor case — the chain is
        // compromised regardless of "why" the author isn't authorized.
        let chain = vec![
            node(
                KEYRING_HEAD_BOB,
                keyring(
                    vec![member(DID_A, Role::Manager), member(DID_B, Role::Manager)],
                    Some(KEYRING_GENESIS),
                ),
            ),
            node(
                KEYRING_GENESIS,
                keyring(vec![member(DID_A, Role::Manager)], None),
            ),
        ];
        let err = verify_keyring_chain_authority(&chain).unwrap_err();
        match err {
            Error::ChainAuthorityViolation { uri, author_did } => {
                assert_eq!(uri, KEYRING_HEAD_BOB);
                assert_eq!(author_did, DID_B);
            }
            other => panic!("expected ChainAuthorityViolation, got: {other:?}"),
        }
    }

    // spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    #[test]
    fn accepts_multi_hop_chain_with_proper_authority() {
        // Three-record chain: each supersede authored by a manager in
        // the immediately prior keyring. Walks the full chain to verify
        // the loop doesn't accidentally skip intermediate pairs.
        let middle_uri = "at://did:plc:alice/at.opake.keyring/middle";
        let chain = vec![
            node(
                KEYRING_HEAD,
                keyring(vec![member(DID_A, Role::Manager)], Some(middle_uri)),
            ),
            node(
                middle_uri,
                keyring(
                    vec![member(DID_A, Role::Manager), member(DID_B, Role::Editor)],
                    Some(KEYRING_GENESIS),
                ),
            ),
            node(
                KEYRING_GENESIS,
                keyring(vec![member(DID_A, Role::Manager)], None),
            ),
        ];
        verify_keyring_chain_authority(&chain).unwrap();
    }

    // spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    #[test]
    fn rejects_when_break_is_mid_chain() {
        // Three-record chain where the middle supersede is authored by
        // someone who became a manager only later. At supersede time
        // they weren't a manager → reject. The check must catch this
        // even though the head's author IS a manager.
        let middle_uri = "at://did:plc:bob/at.opake.keyring/middle";
        let chain = vec![
            node(
                KEYRING_HEAD,
                keyring(
                    vec![member(DID_A, Role::Manager), member(DID_B, Role::Manager)],
                    Some(middle_uri),
                ),
            ),
            node(
                middle_uri,
                keyring(
                    vec![member(DID_A, Role::Manager), member(DID_B, Role::Manager)],
                    Some(KEYRING_GENESIS),
                ),
            ),
            node(
                KEYRING_GENESIS,
                keyring(
                    vec![member(DID_A, Role::Manager), member(DID_B, Role::Editor)],
                    None,
                ),
            ),
        ];
        let err = verify_keyring_chain_authority(&chain).unwrap_err();
        match err {
            Error::ChainAuthorityViolation { uri, author_did } => {
                assert_eq!(uri, middle_uri);
                assert_eq!(author_did, DID_B);
            }
            other => panic!("expected ChainAuthorityViolation, got: {other:?}"),
        }
    }
}

// -- verify_directory_additivity --
//
// Pure check: editor-authored supersedes must add to the prior
// canonical's entry set; managers are exempt.

mod directory_additivity {
    use super::*;
    use crate::records::{Directory, ListingEntry};

    const ALICE_DID: &str = "did:plc:alice";
    const BOB_DID: &str = "did:plc:bob";
    const DIR_GENESIS: &str = "at://did:plc:alice/at.opake.directory/genesis";
    const DIR_HEAD_BY_BOB: &str = "at://did:plc:bob/at.opake.directory/head";

    fn entry(target: &str) -> ListingEntry {
        // Minimal listing entry — only `target` matters for additivity.
        ListingEntry {
            target: target.into(),
            target_cid: crate::records::CidLink {
                cid: "bafyfake".into(),
            },
        }
    }

    fn dir(entries: Vec<&str>, supersedes: Option<&str>) -> Directory {
        Directory {
            opake_version: crate::records::SCHEMA_VERSION,
            key_wrapping: crate::records::KeyWrapping::Direct(crate::records::DirectKeyWrapping {
                keys: vec![],
            }),
            encrypted_metadata: crate::test_utils::dummy_encrypted_metadata(),
            entries: entries.into_iter().map(entry).collect(),
            supersedes: supersedes.map(String::from),
            lineage: supersedes.map(|_| DIR_GENESIS.to_string()),
            workspace_id: None,
            is_workspace_root: false,
            created_at: "2026-03-01T00:00:00Z".into(),
            modified_at: None,
        }
    }

    fn no_managers(_: &str) -> bool {
        false
    }

    fn alice_is_manager(did: &str) -> bool {
        did == ALICE_DID
    }

    /// No supersede links — the legacy "strict superset" behavior.
    fn none_supersedes(_: &str) -> Option<String> {
        None
    }

    // spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    #[test]
    fn passes_genesis_only() {
        let records = vec![(DIR_GENESIS.into(), dir(vec!["doc1", "doc2"], None))];
        verify_directory_additivity(&records, no_managers, none_supersedes).unwrap();
    }

    // spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    #[test]
    fn passes_editor_additive_supersede() {
        // Bob (editor) adds "doc3" while preserving doc1 + doc2.
        let records = vec![
            (DIR_GENESIS.into(), dir(vec!["doc1", "doc2"], None)),
            (
                DIR_HEAD_BY_BOB.into(),
                dir(vec!["doc1", "doc2", "doc3"], Some(DIR_GENESIS)),
            ),
        ];
        verify_directory_additivity(&records, no_managers, none_supersedes).unwrap();
    }

    // spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    #[test]
    fn rejects_editor_non_additive_supersede() {
        // Bob (editor) supersedes but drops doc2. Should reject.
        let records = vec![
            (DIR_GENESIS.into(), dir(vec!["doc1", "doc2"], None)),
            (
                DIR_HEAD_BY_BOB.into(),
                dir(vec!["doc1", "doc3"], Some(DIR_GENESIS)),
            ),
        ];
        let err = verify_directory_additivity(&records, no_managers, none_supersedes).unwrap_err();
        match err {
            Error::ChainAdditivityViolation {
                uri,
                author_did,
                missing,
            } => {
                assert_eq!(uri, DIR_HEAD_BY_BOB);
                assert_eq!(author_did, BOB_DID);
                assert_eq!(missing, vec!["doc2".to_string()]);
            }
            other => panic!("expected ChainAdditivityViolation, got: {other:?}"),
        }
    }

    // spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    #[test]
    fn allows_manager_non_additive_supersede() {
        // Alice (manager) deletes doc2. Allowed because managers are
        // exempt from the additivity rule.
        let records = vec![
            (DIR_GENESIS.into(), dir(vec!["doc1", "doc2"], None)),
            (
                "at://did:plc:alice/at.opake.directory/head".into(),
                dir(vec!["doc1"], Some(DIR_GENESIS)),
            ),
        ];
        verify_directory_additivity(&records, alice_is_manager, none_supersedes).unwrap();
    }

    // spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    #[test]
    fn skips_supersedes_with_missing_prior() {
        // If the prior isn't in the snapshot, we can't verify — skip
        // rather than reject (the indexer would surface this elsewhere).
        let records = vec![(
            DIR_HEAD_BY_BOB.into(),
            dir(
                vec!["doc1"],
                Some("at://did:plc:other/at.opake.directory/gone"),
            ),
        )];
        verify_directory_additivity(&records, no_managers, none_supersedes).unwrap();
    }

    // spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    #[test]
    fn checks_every_supersede_in_chain() {
        // Three-record chain. Middle supersede is non-additive — must be
        // caught even though the head IS additive vs. the middle.
        let middle_uri = "at://did:plc:bob/at.opake.directory/middle";
        let records = vec![
            (DIR_GENESIS.into(), dir(vec!["doc1", "doc2", "doc3"], None)),
            (
                middle_uri.into(),
                dir(vec!["doc1", "doc2"], Some(DIR_GENESIS)),
            ),
            (
                DIR_HEAD_BY_BOB.into(),
                dir(vec!["doc1", "doc2"], Some(middle_uri)),
            ),
        ];
        let err = verify_directory_additivity(&records, no_managers, none_supersedes).unwrap_err();
        match err {
            Error::ChainAdditivityViolation { uri, missing, .. } => {
                assert_eq!(uri, middle_uri);
                assert_eq!(missing, vec!["doc3".to_string()]);
            }
            other => panic!("expected ChainAdditivityViolation, got: {other:?}"),
        }
    }

    // spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    #[test]
    fn passes_editor_advance_when_dropped_entry_is_superseded() {
        // Bob (editor) edits doc2 → doc2b, where doc2b supersedes doc2. The
        // entry set drops doc2 and adds doc2b — non-additive on its face, but
        // the supersede link makes it a legitimate advance.
        let records = vec![
            (DIR_GENESIS.into(), dir(vec!["doc1", "doc2"], None)),
            (
                DIR_HEAD_BY_BOB.into(),
                dir(vec!["doc1", "doc2b"], Some(DIR_GENESIS)),
            ),
        ];
        let supersedes_of = |uri: &str| (uri == "doc2b").then(|| "doc2".to_string());
        verify_directory_additivity(&records, no_managers, supersedes_of).unwrap();
    }

    // spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    #[test]
    fn rejects_editor_substitute_that_supersedes_wrong_entry() {
        // doc2 dropped, doc9 added, but doc9 supersedes some unrelated docX —
        // doc2 is uncovered, so this is a delete dressed as an edit.
        let records = vec![
            (DIR_GENESIS.into(), dir(vec!["doc1", "doc2"], None)),
            (
                DIR_HEAD_BY_BOB.into(),
                dir(vec!["doc1", "doc9"], Some(DIR_GENESIS)),
            ),
        ];
        let supersedes_of = |uri: &str| (uri == "doc9").then(|| "docX".to_string());
        let err = verify_directory_additivity(&records, no_managers, supersedes_of).unwrap_err();
        match err {
            Error::ChainAdditivityViolation { missing, .. } => {
                assert_eq!(missing, vec!["doc2".to_string()]);
            }
            other => panic!("expected ChainAdditivityViolation, got: {other:?}"),
        }
    }

    // spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    #[test]
    fn rejects_editor_bare_delete_even_with_unrelated_supersedes() {
        // doc2 simply removed; nothing added supersedes it. Still a delete.
        let records = vec![
            (DIR_GENESIS.into(), dir(vec!["doc1", "doc2"], None)),
            (DIR_HEAD_BY_BOB.into(), dir(vec!["doc1"], Some(DIR_GENESIS))),
        ];
        let err = verify_directory_additivity(&records, no_managers, none_supersedes).unwrap_err();
        assert!(matches!(err, Error::ChainAdditivityViolation { .. }));
    }
}

// -- ChainHeadProvider contract --
//
// The production impl lives in the indexer-client layer. The map-backed
// mock below exercises the trait shape and contract.

#[derive(Default)]
struct MapChainHeadProvider {
    heads: std::collections::HashMap<String, WorkspaceChainHeads>,
}

impl ChainHeadProvider for MapChainHeadProvider {
    async fn workspace_chain_heads(
        &self,
        workspace_id: &crate::workspace::WorkspaceId,
    ) -> Result<WorkspaceChainHeads, Error> {
        Ok(self
            .heads
            .get(workspace_id.as_str())
            .cloned()
            .unwrap_or_default())
    }
}

#[tokio::test]
async fn chain_head_provider_returns_empty_for_unknown_workspace() {
    let provider = MapChainHeadProvider::default();
    let heads = provider
        .workspace_chain_heads(&crate::workspace::WorkspaceId::from_resolved("ws-missing"))
        .await
        .unwrap();
    assert!(heads.keyring.is_none());
    assert!(heads.root_directory.is_none());
}

#[tokio::test]
async fn chain_head_provider_returns_known_heads() {
    let mut heads = std::collections::HashMap::new();
    heads.insert(
        "ws-1".to_owned(),
        WorkspaceChainHeads {
            keyring: Some(ChainHead {
                uri: URI_HEAD.to_owned(),
                cid: "bafykeyring".to_owned(),
            }),
            root_directory: Some(ChainHead {
                uri: URI_GENESIS.to_owned(),
                cid: "bafyroot".to_owned(),
            }),
        },
    );
    let provider = MapChainHeadProvider { heads };

    let result = provider
        .workspace_chain_heads(&crate::workspace::WorkspaceId::from_resolved("ws-1"))
        .await
        .unwrap();
    assert_eq!(result.keyring.as_ref().unwrap().uri, URI_HEAD);
    assert_eq!(result.root_directory.as_ref().unwrap().cid, "bafyroot");
}

#[tokio::test]
async fn chain_head_provider_handles_partial_population() {
    // A freshly-created workspace has a keyring head but no root yet.
    let mut heads = std::collections::HashMap::new();
    heads.insert(
        "ws-fresh".to_owned(),
        WorkspaceChainHeads {
            keyring: Some(ChainHead {
                uri: URI_GENESIS.to_owned(),
                cid: "bafygenesis".to_owned(),
            }),
            root_directory: None,
        },
    );
    let provider = MapChainHeadProvider { heads };

    let result = provider
        .workspace_chain_heads(&crate::workspace::WorkspaceId::from_resolved("ws-fresh"))
        .await
        .unwrap();
    assert!(result.keyring.is_some());
    assert!(result.root_directory.is_none());
}
