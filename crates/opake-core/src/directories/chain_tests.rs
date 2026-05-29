use super::*;
use crate::client::HttpResponse;
use crate::records::Directory;
use crate::test_utils::MockTransport;

use super::super::tests::{dummy_directory, dummy_directory_with_entries};

const DID_A: &str = "did:plc:alice";
const DID_B: &str = "did:plc:bob";
const PDS_A: &str = "https://pds.alice.example.com";
const PDS_B: &str = "https://pds.bob.example.com";

const URI_GENESIS: &str = "at://did:plc:alice/app.opake.directory/genesis";
const URI_MIDDLE: &str = "at://did:plc:bob/app.opake.directory/middle";
const URI_HEAD: &str = "at://did:plc:alice/app.opake.directory/head";

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
    let dir = dummy_directory_with_entries("/", vec!["at://did:plc:x/app.opake.document/d1".into()]);

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_GENESIS, "bafygenesis", &dir)));

    let node: ChainNode<Directory> = fetch_chain_node(&mock, URI_GENESIS).await.unwrap();

    assert_eq!(node.uri, URI_GENESIS);
    assert_eq!(node.cid, "bafygenesis");
    assert_eq!(node.record.entries.len(), 1);

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2);
    assert!(reqs[0].url.contains(DID_A), "first request hits PLC for DID resolution");
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
    assert!(mock.requests().is_empty(), "no network call on malformed URI");
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

// -- walk_back_to_genesis --

#[tokio::test]
async fn walk_back_returns_genesis_only_when_no_supersedes() {
    let mock = MockTransport::new();
    let genesis = dummy_directory("/");

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_GENESIS, "bafygenesis", &genesis)));

    let chain: Vec<ChainNode<Directory>> =
        walk_back_to_genesis(&mock, URI_GENESIS).await.unwrap();

    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].uri, URI_GENESIS);
    assert!(chain[0].record.supersedes.is_none());
}

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

    let chain: Vec<ChainNode<Directory>> =
        walk_back_to_genesis(&mock, URI_HEAD).await.unwrap();

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

#[tokio::test]
async fn verify_and_walk_chain_accepts_matching_genesis() {
    let mock = MockTransport::new();
    let genesis = dummy_directory("/");
    let head = dir_superseding("/", URI_GENESIS);

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_HEAD, "bafyhead", &head)));
    mock.enqueue(ok(record_entry(URI_GENESIS, "bafygenesis", &genesis)));

    let chain: Vec<ChainNode<Directory>> =
        verify_and_walk_chain(&mock, URI_HEAD, URI_GENESIS).await.unwrap();

    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].uri, URI_HEAD);
    assert_eq!(chain[1].uri, URI_GENESIS);
}

#[tokio::test]
async fn verify_and_walk_chain_accepts_genesis_only_chain() {
    // Freshly-created workspace: head and genesis are the same record.
    let mock = MockTransport::new();
    let genesis = dummy_directory("/");

    mock.enqueue(ok(did_doc(DID_A, PDS_A)));
    mock.enqueue(ok(record_entry(URI_GENESIS, "bafygenesis", &genesis)));

    let chain: Vec<ChainNode<Directory>> =
        verify_and_walk_chain(&mock, URI_GENESIS, URI_GENESIS).await.unwrap();

    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].uri, URI_GENESIS);
    assert!(chain[0].record.supersedes.is_none());
}

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

    let other_workspace = "at://did:plc:other/app.opake.directory/elsewhere";
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
        workspace_id: &str,
    ) -> Result<WorkspaceChainHeads, Error> {
        Ok(self.heads.get(workspace_id).cloned().unwrap_or_default())
    }
}

#[tokio::test]
async fn chain_head_provider_returns_empty_for_unknown_workspace() {
    let provider = MapChainHeadProvider::default();
    let heads = provider.workspace_chain_heads("ws-missing").await.unwrap();
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

    let result = provider.workspace_chain_heads("ws-1").await.unwrap();
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

    let result = provider.workspace_chain_heads("ws-fresh").await.unwrap();
    assert!(result.keyring.is_some());
    assert!(result.root_directory.is_none());
}
