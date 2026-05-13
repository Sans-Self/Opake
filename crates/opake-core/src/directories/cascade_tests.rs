use super::*;
use crate::client::{HttpResponse, RequestBody};
use crate::records::{Directory, ListingEntry};
use crate::test_utils::MockTransport;

use super::super::tests::{dummy_directory, dummy_directory_with_entries, mock_client, TEST_DID};

const DOC_URI: &str = "at://did:plc:test/app.opake.document/doc1";
const DOC_CID: &str = "bafydoc1";

fn ok_create(uri: &str, cid: &str) -> HttpResponse {
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

fn dir_uri(rkey: &str) -> String {
    format!("at://{TEST_DID}/app.opake.directory/{rkey}")
}

fn supersede_mode(prior_uri: &str, seed: &Directory) -> LevelMode {
    LevelMode::Supersede {
        prior_head_uri: prior_uri.to_owned(),
        key_wrapping: seed.key_wrapping.clone(),
        encrypted_metadata: seed.encrypted_metadata.clone(),
    }
}

fn genesis_mode(seed: &Directory, rkey: Option<String>) -> LevelMode {
    LevelMode::Genesis {
        key_wrapping: seed.key_wrapping.clone(),
        encrypted_metadata: seed.encrypted_metadata.clone(),
        rkey,
    }
}

#[tokio::test]
async fn single_level_supersede_writes_leaf_and_returns_head() {
    let mock = MockTransport::new();
    let prior_root_uri = dir_uri("rootOld");
    let new_root_uri = dir_uri("rootNew");

    let prior_root = dummy_directory("/");
    let leaf_entries = vec![ListingEntry::new(DOC_URI, DOC_CID)];

    mock.enqueue(ok_create(&new_root_uri, "bafyrootNew"));

    let mut client = mock_client(mock.clone());
    let leaf = LeafLevel {
        mode: supersede_mode(&prior_root_uri, &prior_root),
        entries: leaf_entries,
    };

    let outcome = execute_cascade(&mut client, vec![], leaf, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(outcome.steps.len(), 1);
    assert_eq!(outcome.leaf().unwrap().uri, new_root_uri);
    assert_eq!(outcome.leaf().unwrap().cid, "bafyrootNew");
    assert_eq!(
        outcome.steps[0].superseded.as_deref(),
        Some(prior_root_uri.as_str())
    );

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].url.contains("createRecord"));

    match &reqs[0].body {
        Some(RequestBody::Json(v)) => {
            let written: Directory =
                serde_json::from_value(v["record"].clone()).expect("record body");
            assert_eq!(written.supersedes.as_deref(), Some(prior_root_uri.as_str()));
            assert_eq!(written.entries.len(), 1);
            assert_eq!(written.entries[0].target, DOC_URI);
        }
        _ => panic!("expected JSON body"),
    }
}

#[tokio::test]
async fn two_level_supersede_threads_child_cid_into_parent() {
    let mock = MockTransport::new();
    let prior_root_uri = dir_uri("rootOld");
    let prior_leaf_uri = dir_uri("leafOld");
    let new_root_uri = dir_uri("rootNew");
    let new_leaf_uri = dir_uri("leafNew");

    let prior_root = dummy_directory_with_entries("/", vec![prior_leaf_uri.clone()]);
    let prior_leaf = dummy_directory_with_entries("subdir", vec![]);

    let leaf_entries = vec![ListingEntry::new(DOC_URI, DOC_CID)];
    let root_entries = prior_root.entries.clone();

    mock.enqueue(ok_create(&new_leaf_uri, "bafyleafNew"));
    mock.enqueue(ok_create(&new_root_uri, "bafyrootNew"));

    let mut client = mock_client(mock.clone());
    let ancestors = vec![AncestorLevel {
        mode: supersede_mode(&prior_root_uri, &prior_root),
        linkage: AncestorLinkage::Replace {
            prior_child_uri: prior_leaf_uri.clone(),
        },
        entries: root_entries,
    }];
    let leaf = LeafLevel {
        mode: supersede_mode(&prior_leaf_uri, &prior_leaf),
        entries: leaf_entries,
    };

    let outcome = execute_cascade(&mut client, ancestors, leaf, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(outcome.steps.len(), 2);
    assert_eq!(outcome.leaf().unwrap().uri, new_leaf_uri);
    assert_eq!(outcome.root().unwrap().uri, new_root_uri);

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2);

    // First write: the leaf, superseding leafOld.
    match &reqs[0].body {
        Some(RequestBody::Json(v)) => {
            let written: Directory = serde_json::from_value(v["record"].clone()).unwrap();
            assert_eq!(written.supersedes.as_deref(), Some(prior_leaf_uri.as_str()));
            assert_eq!(written.entries.len(), 1);
            assert_eq!(written.entries[0].target, DOC_URI);
        }
        _ => panic!("expected JSON body"),
    }

    // Second write: the root, with its leaf pointer updated to new leaf URI/CID.
    match &reqs[1].body {
        Some(RequestBody::Json(v)) => {
            let written: Directory = serde_json::from_value(v["record"].clone()).unwrap();
            assert_eq!(written.supersedes.as_deref(), Some(prior_root_uri.as_str()));
            assert_eq!(written.entries.len(), 1);
            assert_eq!(written.entries[0].target, new_leaf_uri);
            assert_eq!(written.entries[0].target_cid.cid, "bafyleafNew");
        }
        _ => panic!("expected JSON body"),
    }
}

#[tokio::test]
async fn add_child_appends_new_listing_entry_at_parent() {
    let mock = MockTransport::new();
    let prior_root_uri = dir_uri("rootOld");
    let new_root_uri = dir_uri("rootNew");
    let new_leaf_uri = dir_uri("leafGenesis");

    let prior_root = dummy_directory_with_entries("/", vec![]);
    let leaf_seed = dummy_directory("subdir");

    let leaf_entries = vec![ListingEntry::new(DOC_URI, DOC_CID)];

    mock.enqueue(ok_create(&new_leaf_uri, "bafyleafGenesis"));
    mock.enqueue(ok_create(&new_root_uri, "bafyrootNew"));

    let mut client = mock_client(mock.clone());
    let ancestors = vec![AncestorLevel {
        mode: supersede_mode(&prior_root_uri, &prior_root),
        linkage: AncestorLinkage::Add,
        entries: prior_root.entries.clone(),
    }];
    let leaf = LeafLevel {
        mode: genesis_mode(&leaf_seed, None),
        entries: leaf_entries,
    };

    let outcome = execute_cascade(&mut client, ancestors, leaf, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(outcome.steps.len(), 2);
    assert!(outcome.steps[0].superseded.is_none());
    assert_eq!(
        outcome.steps[1].superseded.as_deref(),
        Some(prior_root_uri.as_str())
    );

    let reqs = mock.requests();
    match &reqs[1].body {
        Some(RequestBody::Json(v)) => {
            let written: Directory = serde_json::from_value(v["record"].clone()).unwrap();
            assert_eq!(written.entries.len(), 1);
            assert_eq!(written.entries[0].target, new_leaf_uri);
            assert_eq!(written.entries[0].target_cid.cid, "bafyleafGenesis");
        }
        _ => panic!("expected JSON body"),
    }
}

#[tokio::test]
async fn genesis_with_stable_rkey_uses_put_record() {
    let mock = MockTransport::new();
    let stable_rkey = "ws-keyring1";
    let written_uri = dir_uri(stable_rkey);
    let leaf_seed = dummy_directory("/");

    mock.enqueue(ok_create(&written_uri, "bafyws"));

    let mut client = mock_client(mock.clone());
    let leaf = LeafLevel {
        mode: genesis_mode(&leaf_seed, Some(stable_rkey.to_owned())),
        entries: vec![],
    };

    execute_cascade(&mut client, vec![], leaf, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].url.contains("putRecord"));
    match &reqs[0].body {
        Some(RequestBody::Json(v)) => {
            assert_eq!(v["rkey"], stable_rkey);
        }
        _ => panic!("expected JSON body"),
    }
}

#[tokio::test]
async fn replace_child_with_missing_prior_uri_errors() {
    let mock = MockTransport::new();
    let prior_root_uri = dir_uri("rootOld");
    let prior_leaf_uri = dir_uri("leafOld");
    let new_leaf_uri = dir_uri("leafNew");

    // Root does NOT contain prior_leaf_uri in entries — should error
    // when the walker tries to patch the child pointer.
    let prior_root = dummy_directory_with_entries("/", vec![dir_uri("someOtherChild")]);
    let prior_leaf = dummy_directory_with_entries("subdir", vec![]);

    mock.enqueue(ok_create(&new_leaf_uri, "bafyleafNew"));

    let mut client = mock_client(mock);
    let ancestors = vec![AncestorLevel {
        mode: supersede_mode(&prior_root_uri, &prior_root),
        linkage: AncestorLinkage::Replace {
            prior_child_uri: prior_leaf_uri.clone(),
        },
        entries: prior_root.entries.clone(),
    }];
    let leaf = LeafLevel {
        mode: supersede_mode(&prior_leaf_uri, &prior_leaf),
        entries: vec![],
    };

    let err = execute_cascade(&mut client, ancestors, leaf, "2026-03-01T12:00:00Z")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("missing expected child URI"));
}
