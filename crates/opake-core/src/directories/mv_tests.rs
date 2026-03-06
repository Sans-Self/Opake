use super::*;
use crate::client::{HttpResponse, RequestBody};
use crate::records::{Directory, Document};
use crate::test_utils::MockTransport;

use super::super::tests::{
    dummy_directory, dummy_directory_with_entries, get_record_response, list_records_response,
    mock_client, put_record_response,
};
use crate::documents::tests::dummy_document;

fn record_response<T: serde::Serialize>(uri: &str, record: &T) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&serde_json::json!({
            "uri": uri,
            "cid": "bafyrecord",
            "value": record,
        }))
        .unwrap(),
    }
}

const ROOT_URI: &str = "at://did:plc:test/app.opake.directory/self";
const DIR_A_URI: &str = "at://did:plc:test/app.opake.directory/dirA";
const DIR_B_URI: &str = "at://did:plc:test/app.opake.directory/dirB";
const DOC_URI: &str = "at://did:plc:test/app.opake.document/doc1";

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

fn source_dir(uri: &str, name: &str, parent_uri: Option<&str>) -> ResolvedPath {
    ResolvedPath {
        uri: uri.to_string(),
        kind: EntryKind::Directory,
        name: name.to_string(),
        parent_uri: parent_uri.map(String::from),
    }
}

// -- move into directory --

#[tokio::test]
async fn move_doc_into_directory() {
    let tree = load_tree_with(&[
        (
            "self",
            dummy_directory_with_entries("/", vec![DOC_URI.into()]),
        ),
        ("dirA", dummy_directory("Photos")),
    ])
    .await;

    let mock = MockTransport::new();
    // remove_entry: get old parent, put old parent
    mock.enqueue(get_record_response(
        ROOT_URI,
        &dummy_directory_with_entries("/", vec![DOC_URI.into()]),
    ));
    mock.enqueue(put_record_response(ROOT_URI));
    // add_entry: get new parent, put new parent
    mock.enqueue(get_record_response(DIR_A_URI, &dummy_directory("Photos")));
    mock.enqueue(put_record_response(DIR_A_URI));

    let mut client = mock_client(mock.clone());
    let source = source_doc(Some(ROOT_URI));
    let dest = MoveDestination::IntoDirectory {
        directory_uri: DIR_A_URI.to_string(),
    };

    let result = move_entry(&mut client, &tree, &source, &dest, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(result.uri, DOC_URI);
    assert!(result.new_name.is_none());

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 4);

    // Old parent should have doc removed
    match &reqs[1].body {
        Some(RequestBody::Json(v)) => {
            let dir: Directory = serde_json::from_value(v["record"].clone()).unwrap();
            assert!(dir.entries.is_empty());
        }
        _ => panic!("expected JSON body"),
    }

    // New parent should have doc added
    match &reqs[3].body {
        Some(RequestBody::Json(v)) => {
            let dir: Directory = serde_json::from_value(v["record"].clone()).unwrap();
            assert_eq!(dir.entries, vec![DOC_URI]);
        }
        _ => panic!("expected JSON body"),
    }
}

#[tokio::test]
async fn move_untracked_doc_into_directory() {
    let tree = load_tree_with(&[
        ("self", dummy_directory("/")),
        ("dirA", dummy_directory("Photos")),
    ])
    .await;

    let mock = MockTransport::new();
    // No remove_entry — doc has no parent. Just add_entry.
    mock.enqueue(get_record_response(DIR_A_URI, &dummy_directory("Photos")));
    mock.enqueue(put_record_response(DIR_A_URI));

    let mut client = mock_client(mock.clone());
    let source = source_doc(None);
    let dest = MoveDestination::IntoDirectory {
        directory_uri: DIR_A_URI.to_string(),
    };

    let result = move_entry(&mut client, &tree, &source, &dest, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(result.uri, DOC_URI);
    assert_eq!(mock.requests().len(), 2);
}

#[tokio::test]
async fn move_into_same_directory_rejected() {
    let tree = load_tree_with(&[(
        "self",
        dummy_directory_with_entries("/", vec![DOC_URI.into()]),
    )])
    .await;

    let mock = MockTransport::new();
    let mut client = mock_client(mock);
    let source = source_doc(Some(ROOT_URI));
    let dest = MoveDestination::IntoDirectory {
        directory_uri: ROOT_URI.to_string(),
    };

    let err = move_entry(&mut client, &tree, &source, &dest, "2026-03-01T12:00:00Z")
        .await
        .unwrap_err();

    assert!(err.to_string().contains("already in that directory"));
}

// -- rename --

#[tokio::test]
async fn rename_document() {
    let tree = load_tree_with(&[("self", dummy_directory("/"))]).await;

    let doc = dummy_document("beach.jpg", 1000, vec![]);
    let mock = MockTransport::new();
    mock.enqueue(record_response(DOC_URI, &doc));
    mock.enqueue(put_record_response(DOC_URI));

    let mut client = mock_client(mock.clone());
    let source = source_doc(Some(ROOT_URI));
    let dest = MoveDestination::Rename {
        new_name: "sunset.jpg".to_string(),
    };

    let result = move_entry(&mut client, &tree, &source, &dest, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(result.uri, DOC_URI);
    assert_eq!(result.new_name.as_deref(), Some("sunset.jpg"));

    let reqs = mock.requests();
    match &reqs[1].body {
        Some(RequestBody::Json(v)) => {
            let updated: Document = serde_json::from_value(v["record"].clone()).unwrap();
            assert_eq!(updated.name, "sunset.jpg");
            assert_eq!(updated.modified_at.unwrap(), "2026-03-01T12:00:00Z");
        }
        _ => panic!("expected JSON body"),
    }
}

#[tokio::test]
async fn rename_directory() {
    let tree = load_tree_with(&[
        (
            "self",
            dummy_directory_with_entries("/", vec![DIR_A_URI.into()]),
        ),
        ("dirA", dummy_directory("Photos")),
    ])
    .await;

    let mock = MockTransport::new();
    mock.enqueue(get_record_response(DIR_A_URI, &dummy_directory("Photos")));
    mock.enqueue(put_record_response(DIR_A_URI));

    let mut client = mock_client(mock.clone());
    let source = source_dir(DIR_A_URI, "Photos", Some(ROOT_URI));
    let dest = MoveDestination::Rename {
        new_name: "Memories".to_string(),
    };

    let result = move_entry(&mut client, &tree, &source, &dest, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(result.uri, DIR_A_URI);
    assert_eq!(result.new_name.as_deref(), Some("Memories"));

    let reqs = mock.requests();
    match &reqs[1].body {
        Some(RequestBody::Json(v)) => {
            let updated: Directory = serde_json::from_value(v["record"].clone()).unwrap();
            assert_eq!(updated.name, "Memories");
        }
        _ => panic!("expected JSON body"),
    }
}

#[tokio::test]
async fn rename_root_directory_rejected() {
    let tree = load_tree_with(&[("self", dummy_directory("/"))]).await;

    let mock = MockTransport::new();
    let mut client = mock_client(mock);
    let source = source_dir(ROOT_URI, "/", None);
    let dest = MoveDestination::Rename {
        new_name: "newroot".to_string(),
    };

    let err = move_entry(&mut client, &tree, &source, &dest, "2026-03-01T12:00:00Z")
        .await
        .unwrap_err();

    assert!(err.to_string().contains("cannot rename the root directory"));
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
