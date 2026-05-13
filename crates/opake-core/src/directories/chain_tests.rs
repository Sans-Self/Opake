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

// -- ChainHeadProvider contract --
//
// The trait has no production impl yet — that lives in the indexer-client
// layer (not built). The map-backed mock below proves the trait shape
// compiles and exercises the contract the production impl must match.

struct MapChainHeadProvider {
    directory: std::collections::HashMap<(String, String), ChainHead>,
    keyring: std::collections::HashMap<String, ChainHead>,
}

impl ChainHeadProvider for MapChainHeadProvider {
    async fn directory_head(
        &self,
        workspace_id: &str,
        path: &str,
    ) -> Result<Option<ChainHead>, Error> {
        Ok(self
            .directory
            .get(&(workspace_id.to_owned(), path.to_owned()))
            .cloned())
    }

    async fn keyring_head(&self, workspace_id: &str) -> Result<Option<ChainHead>, Error> {
        Ok(self.keyring.get(workspace_id).cloned())
    }
}

#[tokio::test]
async fn chain_head_provider_returns_none_for_unknown_path() {
    let provider = MapChainHeadProvider {
        directory: std::collections::HashMap::new(),
        keyring: std::collections::HashMap::new(),
    };
    let head = provider.directory_head("ws-x", "/missing/").await.unwrap();
    assert!(head.is_none());
}

#[tokio::test]
async fn chain_head_provider_returns_known_head() {
    let mut directory = std::collections::HashMap::new();
    directory.insert(
        ("ws-1".to_owned(), "/q1/".to_owned()),
        ChainHead {
            uri: URI_HEAD.to_owned(),
            cid: "bafyhead".to_owned(),
        },
    );
    let provider = MapChainHeadProvider {
        directory,
        keyring: std::collections::HashMap::new(),
    };
    let head = provider
        .directory_head("ws-1", "/q1/")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(head.uri, URI_HEAD);
    assert_eq!(head.cid, "bafyhead");
}
