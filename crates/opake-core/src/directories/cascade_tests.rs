use super::*;
use crate::client::{HttpResponse, RequestBody};
use crate::records::{Directory, ListingEntry};
use crate::test_utils::MockTransport;

use super::super::tests::{dummy_directory, dummy_directory_with_entries, mock_client, TEST_DID};

const DOC_URI: &str = "at://did:plc:test/app.opake.document/doc1";
const DOC_CID: &str = "bafydoc1";
const TEST_WORKSPACE_ID: &str = "at://did:plc:test/app.opake.keyring/genesis";

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
        is_workspace_root: true,
    };

    let outcome = execute_cascade(
        &mut client,
        TEST_WORKSPACE_ID,
        vec![],
        leaf,
        "2026-03-01T12:00:00Z",
    )
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
        is_workspace_root: false,
    }];
    let leaf = LeafLevel {
        mode: supersede_mode(&prior_leaf_uri, &prior_leaf),
        entries: leaf_entries,
        is_workspace_root: false,
    };

    let outcome = execute_cascade(
        &mut client,
        TEST_WORKSPACE_ID,
        ancestors,
        leaf,
        "2026-03-01T12:00:00Z",
    )
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
        is_workspace_root: false,
    }];
    let leaf = LeafLevel {
        mode: genesis_mode(&leaf_seed, None),
        entries: leaf_entries,
        is_workspace_root: false,
    };

    let outcome = execute_cascade(
        &mut client,
        TEST_WORKSPACE_ID,
        ancestors,
        leaf,
        "2026-03-01T12:00:00Z",
    )
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
        is_workspace_root: true,
    };

    execute_cascade(
        &mut client,
        TEST_WORKSPACE_ID,
        vec![],
        leaf,
        "2026-03-01T12:00:00Z",
    )
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
        is_workspace_root: false,
    }];
    let leaf = LeafLevel {
        mode: supersede_mode(&prior_leaf_uri, &prior_leaf),
        entries: vec![],
        is_workspace_root: false,
    };

    let err = execute_cascade(
        &mut client,
        TEST_WORKSPACE_ID,
        ancestors,
        leaf,
        "2026-03-01T12:00:00Z",
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("missing expected child URI"));
}

// ---------------------------------------------------------------------------
// build_deep_cascade_levels
//
// Refetches each level in a uri_chain (root → target) and assembles the
// (Vec<AncestorLevel>, LeafLevel) shape that `execute_cascade` consumes.
// ---------------------------------------------------------------------------

mod deep_cascade_levels {
    use super::*;
    use crate::client::HttpResponse;

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

    fn ok(value: serde_json::Value) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&value).unwrap(),
        }
    }

    fn get_dir_response(uri: &str, cid: &str, dir: &Directory) -> HttpResponse {
        ok(serde_json::json!({
            "uri": uri,
            "cid": cid,
            "value": dir,
        }))
    }

    /// Empty chain is a usage error — caller didn't compute a path.
    #[tokio::test]
    async fn empty_chain_errors_invalid_record() {
        let mock = MockTransport::new();
        let err = build_deep_cascade_levels(&mock, &[], vec![])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("empty uri_chain"), "got: {err}");
    }

    /// Single-element chain (target IS the root). Returns no ancestors;
    /// leaf supersedes the root URI with caller-supplied entries.
    #[tokio::test]
    async fn single_element_chain_produces_only_leaf() {
        let mock = MockTransport::new();
        let root_uri = format!("at://{TEST_DID}/app.opake.directory/self");
        let root_dir = dummy_directory_with_entries("/", vec![]);

        mock.enqueue(ok(did_doc(TEST_DID, "https://pds.test")));
        mock.enqueue(get_dir_response(&root_uri, "bafyroot", &root_dir));

        let new_entries = vec![ListingEntry::new(DOC_URI, DOC_CID)];
        let (ancestors, leaf) =
            build_deep_cascade_levels(&mock, std::slice::from_ref(&root_uri), new_entries.clone())
                .await
                .unwrap();

        assert!(ancestors.is_empty());
        match &leaf.mode {
            LevelMode::Supersede { prior_head_uri, .. } => assert_eq!(prior_head_uri, &root_uri),
            _ => panic!("expected Supersede mode"),
        }
        assert_eq!(leaf.entries.len(), 1);
        assert_eq!(leaf.entries[0].target, DOC_URI);
    }

    /// Two-element chain (root + subdirectory). One ancestor (root)
    /// linked to the subdir via Replace. Leaf supersedes the subdir.
    #[tokio::test]
    async fn two_element_chain_produces_root_ancestor_and_subdir_leaf() {
        let mock = MockTransport::new();
        let root_uri = format!("at://{TEST_DID}/app.opake.directory/self");
        let subdir_uri = format!("at://{TEST_DID}/app.opake.directory/q1");

        let root_dir = dummy_directory_with_entries("/", vec![subdir_uri.clone()]);
        let subdir = dummy_directory_with_entries("q1", vec![]);

        // Fetches happen in chain order — root first, then subdir.
        mock.enqueue(ok(did_doc(TEST_DID, "https://pds.test")));
        mock.enqueue(get_dir_response(&root_uri, "bafyroot", &root_dir));
        mock.enqueue(get_dir_response(&subdir_uri, "bafysubdir", &subdir));

        let new_entries = vec![ListingEntry::new(DOC_URI, DOC_CID)];
        let (ancestors, leaf) =
            build_deep_cascade_levels(&mock, &[root_uri.clone(), subdir_uri.clone()], new_entries)
                .await
                .unwrap();

        assert_eq!(ancestors.len(), 1);

        // Root ancestor supersedes root_uri, linked to the subdir URI
        // (so the cascade walker will patch in the freshly-written
        // subdir URI/CID when threading up).
        match &ancestors[0].mode {
            LevelMode::Supersede { prior_head_uri, .. } => {
                assert_eq!(prior_head_uri, &root_uri);
            }
            _ => panic!("expected Supersede"),
        }
        match &ancestors[0].linkage {
            AncestorLinkage::Replace { prior_child_uri } => {
                assert_eq!(prior_child_uri, &subdir_uri);
            }
            _ => panic!("expected Replace linkage"),
        }
        // Root's prior entries carry over (the walker patches them).
        assert_eq!(ancestors[0].entries.len(), 1);
        assert_eq!(ancestors[0].entries[0].target, subdir_uri);

        // Leaf supersedes the subdir URI with the new entries.
        match &leaf.mode {
            LevelMode::Supersede { prior_head_uri, .. } => {
                assert_eq!(prior_head_uri, &subdir_uri);
            }
            _ => panic!("expected Supersede"),
        }
        assert_eq!(leaf.entries[0].target, DOC_URI);
    }

    /// Three-element chain demonstrates the ancestor chaining: each
    /// ancestor's `prior_child_uri` points at the URI of the level
    /// immediately below.
    #[tokio::test]
    async fn three_element_chain_threads_child_uris_correctly() {
        let mock = MockTransport::new();
        let root_uri = format!("at://{TEST_DID}/app.opake.directory/self");
        let q1_uri = format!("at://{TEST_DID}/app.opake.directory/q1");
        let foo_uri = format!("at://{TEST_DID}/app.opake.directory/foo");

        let root_dir = dummy_directory_with_entries("/", vec![q1_uri.clone()]);
        let q1 = dummy_directory_with_entries("q1", vec![foo_uri.clone()]);
        let foo = dummy_directory_with_entries("foo", vec![]);

        // Each cross-DID hop normally costs a DID-doc resolve, but the
        // chain walker caches resolutions per call. All three URIs share
        // TEST_DID so it's exactly one resolve.
        mock.enqueue(ok(did_doc(TEST_DID, "https://pds.test")));
        mock.enqueue(get_dir_response(&root_uri, "bafyroot", &root_dir));
        mock.enqueue(get_dir_response(&q1_uri, "bafyq1", &q1));
        mock.enqueue(get_dir_response(&foo_uri, "bafyfoo", &foo));

        let new_entries = vec![ListingEntry::new(DOC_URI, DOC_CID)];
        let (ancestors, leaf) = build_deep_cascade_levels(
            &mock,
            &[root_uri.clone(), q1_uri.clone(), foo_uri.clone()],
            new_entries,
        )
        .await
        .unwrap();

        assert_eq!(ancestors.len(), 2);

        // Root ancestor (index 0) links to q1.
        match &ancestors[0].linkage {
            AncestorLinkage::Replace { prior_child_uri } => {
                assert_eq!(prior_child_uri, &q1_uri);
            }
            _ => panic!(),
        }

        // q1 ancestor (index 1) links to foo.
        match &ancestors[1].linkage {
            AncestorLinkage::Replace { prior_child_uri } => {
                assert_eq!(prior_child_uri, &foo_uri);
            }
            _ => panic!(),
        }

        // Leaf supersedes foo with the new doc.
        match &leaf.mode {
            LevelMode::Supersede { prior_head_uri, .. } => {
                assert_eq!(prior_head_uri, &foo_uri);
            }
            _ => panic!(),
        }
    }
}
