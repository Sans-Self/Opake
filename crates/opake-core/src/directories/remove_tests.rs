use super::*;
use crate::client::HttpResponse;
use crate::test_utils::MockTransport;

use super::super::tests::{
    dummy_directory_with_entries, get_record_response, list_records_response, mock_client,
    put_record_response, test_keypair, TEST_DID,
};

const ROOT_URI: &str = "at://did:plc:test/app.opake.directory/self";
const DIR_PHOTOS_URI: &str = "at://did:plc:test/app.opake.directory/photos";
const DIR_VACATION_URI: &str = "at://did:plc:test/app.opake.directory/vacation";
const DOC_BEACH_URI: &str = "at://did:plc:test/app.opake.document/beach";
const DOC_NOTES_URI: &str = "at://did:plc:test/app.opake.document/notes";
const DOC_SUNSET_URI: &str = "at://did:plc:test/app.opake.document/sunset";

fn delete_ok() -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: b"{}".to_vec(),
    }
}

fn doc_record_response(uri: &str, name: &str) -> HttpResponse {
    use crate::documents::tests::dummy_document;
    let doc = dummy_document(name, 100, vec![]);
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&serde_json::json!({
            "uri": uri,
            "cid": "bafydocument",
            "value": doc,
        }))
        .unwrap(),
    }
}

/// Load a simple tree: / → [Photos → [beach.jpg], notes.txt]
async fn setup_simple(
    mock: &MockTransport,
) -> (crate::client::XrpcClient<MockTransport>, DirectoryTree) {
    mock.enqueue(list_records_response(
        &[
            (
                "self",
                dummy_directory_with_entries(
                    "/",
                    vec![DIR_PHOTOS_URI.into(), DOC_NOTES_URI.into()],
                ),
            ),
            (
                "photos",
                dummy_directory_with_entries("Photos", vec![DOC_BEACH_URI.into()]),
            ),
        ],
        None,
    ));

    let mut client = mock_client(mock.clone());
    let mut tree = DirectoryTree::load(&mut client).await.unwrap();
    let (_, private_key) = test_keypair();
    tree.decrypt_names(TEST_DID, &private_key);
    (client, tree)
}

/// Load a nested tree: / → Photos → [Vacation → [sunset.jpg], beach.jpg]
async fn setup_nested(
    mock: &MockTransport,
) -> (crate::client::XrpcClient<MockTransport>, DirectoryTree) {
    mock.enqueue(list_records_response(
        &[
            (
                "self",
                dummy_directory_with_entries("/", vec![DIR_PHOTOS_URI.into()]),
            ),
            (
                "photos",
                dummy_directory_with_entries(
                    "Photos",
                    vec![DIR_VACATION_URI.into(), DOC_BEACH_URI.into()],
                ),
            ),
            (
                "vacation",
                dummy_directory_with_entries("Vacation", vec![DOC_SUNSET_URI.into()]),
            ),
        ],
        None,
    ));

    let mut client = mock_client(mock.clone());
    let mut tree = DirectoryTree::load(&mut client).await.unwrap();
    let (_, private_key) = test_keypair();
    tree.decrypt_names(TEST_DID, &private_key);
    (client, tree)
}

// -- document removal --

#[tokio::test]
async fn remove_document_with_parent() {
    let mock = MockTransport::new();
    let (mut client, tree) = setup_simple(&mock).await;

    // resolve "Photos/beach.jpg": getRecord for beach.jpg
    mock.enqueue(doc_record_response(DOC_BEACH_URI, "beach.jpg"));
    let resolved = tree.resolve(&mut client, "Photos/beach.jpg").await.unwrap();

    // delete_record for the document
    mock.enqueue(delete_ok());
    // get_record + put_record for remove_entry on parent
    let photos = dummy_directory_with_entries("Photos", vec![DOC_BEACH_URI.into()]);
    mock.enqueue(get_record_response(DIR_PHOTOS_URI, &photos));
    mock.enqueue(put_record_response(DIR_PHOTOS_URI));

    let result = remove(&mut client, &tree, &resolved, false, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(result.documents_deleted, 1);
    assert_eq!(result.directories_deleted, 0);
}

#[tokio::test]
async fn remove_document_without_parent() {
    // Simulate a document resolved via the CLI fast path (no parent tracking).
    let mock = MockTransport::new();
    mock.enqueue(list_records_response(&[], None));

    let mut client = mock_client(mock.clone());
    let tree = DirectoryTree::load(&mut client).await.unwrap();

    let resolved = ResolvedPath {
        uri: "at://did:plc:test/app.opake.document/orphan".into(),
        kind: EntryKind::Document,
        name: "orphan.txt".into(),
        parent_uri: None,
    };

    // delete_record only — no parent to update
    mock.enqueue(delete_ok());

    let result = remove(&mut client, &tree, &resolved, false, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(result.documents_deleted, 1);
    assert_eq!(result.directories_deleted, 0);
}

// -- empty directory removal --

#[tokio::test]
async fn remove_empty_directory() {
    let mock = MockTransport::new();
    let root = dummy_directory_with_entries(
        "/",
        vec!["at://did:plc:test/app.opake.directory/empty".into()],
    );
    mock.enqueue(list_records_response(
        &[
            ("self", root.clone()),
            ("empty", dummy_directory_with_entries("Empty", vec![])),
        ],
        None,
    ));

    let mut client = mock_client(mock.clone());
    let mut tree = DirectoryTree::load(&mut client).await.unwrap();
    let (_, private_key) = test_keypair();
    tree.decrypt_names(TEST_DID, &private_key);

    let resolved = tree.resolve(&mut client, "Empty").await.unwrap();

    // delete_record for the directory
    mock.enqueue(delete_ok());
    // get_record + put_record for remove_entry on root
    mock.enqueue(get_record_response(ROOT_URI, &root));
    mock.enqueue(put_record_response(ROOT_URI));

    let result = remove(&mut client, &tree, &resolved, false, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(result.documents_deleted, 0);
    assert_eq!(result.directories_deleted, 1);
}

// -- non-empty directory without -r --

#[tokio::test]
async fn remove_nonempty_without_recursive_errors() {
    let mock = MockTransport::new();
    let (mut client, tree) = setup_simple(&mock).await;

    // resolve "Photos": directory, found in memory. But find_child_any
    // also scans document children for ambiguity.
    mock.enqueue(doc_record_response(DOC_NOTES_URI, "notes.txt"));
    let resolved = tree.resolve(&mut client, "Photos").await.unwrap();

    let err = remove(&mut client, &tree, &resolved, false, "2026-03-01T12:00:00Z")
        .await
        .unwrap_err();

    let msg = err.to_string();
    assert!(msg.contains("not empty"), "got: {msg}");
    assert!(msg.contains("-r"), "should suggest -r, got: {msg}");
}

// -- recursive directory removal --

#[tokio::test]
async fn remove_recursive_flat() {
    let mock = MockTransport::new();
    let (mut client, tree) = setup_simple(&mock).await;

    // resolve "Photos"
    mock.enqueue(doc_record_response(DOC_NOTES_URI, "notes.txt"));
    let resolved = tree.resolve(&mut client, "Photos").await.unwrap();

    // delete beach.jpg (descendant document)
    mock.enqueue(delete_ok());
    // delete Photos directory
    mock.enqueue(delete_ok());
    // get_record + put_record for remove_entry on root
    let root = dummy_directory_with_entries("/", vec![DIR_PHOTOS_URI.into(), DOC_NOTES_URI.into()]);
    mock.enqueue(get_record_response(ROOT_URI, &root));
    mock.enqueue(put_record_response(ROOT_URI));

    let result = remove(&mut client, &tree, &resolved, true, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(result.documents_deleted, 1);
    assert_eq!(result.directories_deleted, 1);
}

#[tokio::test]
async fn remove_recursive_nested() {
    let mock = MockTransport::new();
    let (mut client, tree) = setup_nested(&mock).await;

    // resolve "Photos": directory, found in memory.
    // find_child_any scans root's document children — but root has none.
    let resolved = tree.resolve(&mut client, "Photos").await.unwrap();

    // Post-order: sunset.jpg, Vacation, beach.jpg, then Photos itself
    mock.enqueue(delete_ok()); // sunset.jpg
    mock.enqueue(delete_ok()); // Vacation
    mock.enqueue(delete_ok()); // beach.jpg
    mock.enqueue(delete_ok()); // Photos
                               // get_record + put_record for remove_entry on root
    let root = dummy_directory_with_entries("/", vec![DIR_PHOTOS_URI.into()]);
    mock.enqueue(get_record_response(ROOT_URI, &root));
    mock.enqueue(put_record_response(ROOT_URI));

    let result = remove(&mut client, &tree, &resolved, true, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(result.documents_deleted, 2); // sunset + beach
    assert_eq!(result.directories_deleted, 2); // Vacation + Photos
}

// -- root deletion guard --

#[tokio::test]
async fn remove_root_rejected() {
    let mock = MockTransport::new();
    let (mut client, tree) = setup_simple(&mock).await;

    let resolved = tree.resolve_at_uri(&mut client, ROOT_URI).await.unwrap();

    let err = remove(&mut client, &tree, &resolved, true, "2026-03-01T12:00:00Z")
        .await
        .unwrap_err();

    assert!(err.to_string().contains("root directory"));
}
