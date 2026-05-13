use super::*;
use crate::client::{HttpResponse, RequestBody};
use crate::records::Directory;
use crate::test_utils::MockTransport;

use super::super::tests::{
    dummy_directory, dummy_directory_with_entries, get_record_response, list_records_response,
    mock_client, put_record_response,
};

const ROOT_URI: &str = "at://did:plc:test/app.opake.directory/self";
const DIR_A_URI: &str = "at://did:plc:test/app.opake.directory/dirA";
const DIR_B_URI: &str = "at://did:plc:test/app.opake.directory/dirB";
const DOC_URI: &str = "at://did:plc:test/app.opake.document/doc1";
const DOC_CID: &str = "bafytestdoccid";

fn doc_record_response() -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&serde_json::json!({
            "uri": DOC_URI,
            "cid": DOC_CID,
            "value": {},
        }))
        .unwrap(),
    }
}

async fn load_tree_with(dirs: &[(&str, Directory)]) -> DirectoryTree {
    let mock = MockTransport::new();
    mock.enqueue(list_records_response(dirs, None));
    let mut client = mock_client(mock);
    DirectoryTree::load(&mut client).await.unwrap()
}

fn source_doc(parent_uri: Option<&str>) -> ResolvedPath {
    ResolvedPath {
        uri: DOC_URI.to_string(),
        kind: EntryKind::Document,
        name: "beach.jpg".to_string(),
        parent_uri: parent_uri.map(String::from),
    }
}

// -- move into directory --

#[tokio::test]
async fn move_doc_into_directory() {
    let _tree = load_tree_with(&[
        (
            "self",
            dummy_directory_with_entries("/", vec![DOC_URI.into()]),
        ),
        ("dirA", dummy_directory("Photos")),
    ])
    .await;

    let mock = MockTransport::new();
    // remove_entry: getRecord(old parent), applyWrites
    mock.enqueue(get_record_response(
        ROOT_URI,
        &dummy_directory_with_entries("/", vec![DOC_URI.into()]),
    ));
    mock.enqueue(put_record_response(ROOT_URI));
    // move_entry fetches the doc's CID before add_entry
    mock.enqueue(doc_record_response());
    // add_entry: getRecord(new parent), applyWrites
    mock.enqueue(get_record_response(DIR_A_URI, &dummy_directory("Photos")));
    mock.enqueue(put_record_response(DIR_A_URI));

    let mut client = mock_client(mock.clone());
    let source = source_doc(Some(ROOT_URI));

    let result = move_entry(&mut client, &source, DIR_A_URI, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(result.uri, DOC_URI);

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 5);

    // Old parent should have doc removed (via applyWrites)
    assert!(reqs[1].url.contains("applyWrites"));
    match &reqs[1].body {
        Some(RequestBody::Json(v)) => {
            let writes = v["writes"].as_array().unwrap();
            let dir: Directory = serde_json::from_value(writes[0]["value"].clone()).unwrap();
            assert!(dir.entries.is_empty());
        }
        _ => panic!("expected JSON body"),
    }

    // New parent should have doc added with the observed CID
    assert!(reqs[4].url.contains("applyWrites"));
    match &reqs[4].body {
        Some(RequestBody::Json(v)) => {
            let writes = v["writes"].as_array().unwrap();
            let dir: Directory = serde_json::from_value(writes[0]["value"].clone()).unwrap();
            assert_eq!(dir.entries.len(), 1);
            assert_eq!(dir.entries[0].target, DOC_URI);
            assert_eq!(dir.entries[0].target_cid.cid, DOC_CID);
        }
        _ => panic!("expected JSON body"),
    }
}

#[tokio::test]
async fn move_untracked_doc_into_directory() {
    let _tree = load_tree_with(&[
        ("self", dummy_directory("/")),
        ("dirA", dummy_directory("Photos")),
    ])
    .await;

    let mock = MockTransport::new();
    // No remove_entry — doc has no parent. Fetch doc CID, then add_entry.
    mock.enqueue(doc_record_response());
    mock.enqueue(get_record_response(DIR_A_URI, &dummy_directory("Photos")));
    mock.enqueue(put_record_response(DIR_A_URI));

    let mut client = mock_client(mock.clone());
    let source = source_doc(None);

    let result = move_entry(&mut client, &source, DIR_A_URI, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(result.uri, DOC_URI);
    assert_eq!(mock.requests().len(), 3);
}

#[tokio::test]
async fn move_into_same_directory_rejected() {
    let _tree = load_tree_with(&[(
        "self",
        dummy_directory_with_entries("/", vec![DOC_URI.into()]),
    )])
    .await;

    let mock = MockTransport::new();
    let mut client = mock_client(mock);
    let source = source_doc(Some(ROOT_URI));

    let err = move_entry(&mut client, &source, ROOT_URI, "2026-03-01T12:00:00Z")
        .await
        .unwrap_err();

    assert!(err.to_string().contains("already in that directory"));
}

// -- cycle detection --

#[tokio::test]
async fn cycle_self_reference() {
    let tree = load_tree_with(&[
        (
            "self",
            dummy_directory_with_entries("/", vec![DIR_A_URI.into()]),
        ),
        ("dirA", dummy_directory("Photos")),
    ])
    .await;

    let err = check_cycle(&tree, DIR_A_URI, DIR_A_URI).unwrap_err();
    assert!(err.to_string().contains("into itself"));
}

#[tokio::test]
async fn cycle_into_descendant() {
    let tree = load_tree_with(&[
        (
            "self",
            dummy_directory_with_entries("/", vec![DIR_A_URI.into()]),
        ),
        (
            "dirA",
            dummy_directory_with_entries("Photos", vec![DIR_B_URI.into()]),
        ),
        ("dirB", dummy_directory("Archive")),
    ])
    .await;

    let err = check_cycle(&tree, DIR_A_URI, DIR_B_URI).unwrap_err();
    assert!(err.to_string().contains("descendants"));
}

#[tokio::test]
async fn no_cycle_for_sibling() {
    let tree = load_tree_with(&[
        (
            "self",
            dummy_directory_with_entries("/", vec![DIR_A_URI.into(), DIR_B_URI.into()]),
        ),
        ("dirA", dummy_directory("Photos")),
        ("dirB", dummy_directory("Archive")),
    ])
    .await;

    check_cycle(&tree, DIR_A_URI, DIR_B_URI).unwrap();
}
