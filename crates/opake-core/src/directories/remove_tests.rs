use std::collections::HashMap;

use super::*;
use crate::client::HttpResponse;
use crate::test_utils::MockTransport;

use super::super::tests::{
    dummy_directory_with_entries, get_record_response, list_records_response, mock_client,
    put_record_response, test_keypair, TEST_DID,
};

use super::super::tree::DocumentNameResolver;

/// Test resolver that returns names from a pre-built map.
struct MockNameResolver {
    names: HashMap<String, String>,
}

impl MockNameResolver {
    fn new(pairs: &[(&str, &str)]) -> Self {
        Self {
            names: pairs
                .iter()
                .map(|(uri, name)| (uri.to_string(), name.to_string()))
                .collect(),
        }
    }
}

impl DocumentNameResolver for MockNameResolver {
    async fn resolve_name(&mut self, uri: &str) -> Result<Option<String>, crate::error::Error> {
        Ok(self.names.get(uri).cloned())
    }
}

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
    let kp = test_keypair();
    tree.decrypt_names(TEST_DID, &kp.private_keys());
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
    let kp = test_keypair();
    tree.decrypt_names(TEST_DID, &kp.private_keys());
    (client, tree)
}

// -- document removal --

#[tokio::test]
async fn remove_document_with_parent() {
    let mock = MockTransport::new();
    let (mut client, tree) = setup_simple(&mock).await;

    let mut resolver =
        MockNameResolver::new(&[(DOC_BEACH_URI, "beach.jpg"), (DOC_NOTES_URI, "notes.txt")]);
    let resolved = tree
        .resolve(&mut resolver, "Photos/beach.jpg")
        .await
        .unwrap();

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
    let kp = test_keypair();
    tree.decrypt_names(TEST_DID, &kp.private_keys());

    let mut resolver = MockNameResolver::new(&[]);
    let resolved = tree.resolve(&mut resolver, "Empty").await.unwrap();

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

    let mut resolver = MockNameResolver::new(&[(DOC_NOTES_URI, "notes.txt")]);
    let resolved = tree.resolve(&mut resolver, "Photos").await.unwrap();

    let err = remove(&mut client, &tree, &resolved, false, "2026-03-01T12:00:00Z")
        .await
        .unwrap_err();

    // spec:tree-cabinet § Deletion removes target records before the parent listing entry
    let msg = err.to_string();
    assert!(msg.contains("not empty"), "got: {msg}");
    assert!(msg.contains("-r"), "should suggest -r, got: {msg}");
}

// -- recursive directory removal --

#[tokio::test]
async fn remove_recursive_flat() {
    let mock = MockTransport::new();
    let (mut client, tree) = setup_simple(&mock).await;

    let mut resolver = MockNameResolver::new(&[(DOC_NOTES_URI, "notes.txt")]);
    let resolved = tree.resolve(&mut resolver, "Photos").await.unwrap();

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

    let mut resolver = MockNameResolver::new(&[]);
    let resolved = tree.resolve(&mut resolver, "Photos").await.unwrap();

    // Post-order: sunset.jpg, Vacation, beach.jpg, then Photos itself —
    // descendant records before the parent, parent listing updated last.
    // spec:tree-cabinet § Deletion removes target records before the parent listing entry
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

// -- root deletion --

#[tokio::test]
async fn remove_root_without_recursive_rejected() {
    let mock = MockTransport::new();
    let (mut client, tree) = setup_simple(&mock).await;

    let mut resolver = MockNameResolver::new(&[]);
    let resolved = tree.resolve(&mut resolver, ROOT_URI).await.unwrap();

    let err = remove(&mut client, &tree, &resolved, false, "2026-03-01T12:00:00Z")
        .await
        .unwrap_err();

    assert!(err.to_string().contains("root directory"));
    assert!(err.to_string().contains("-r"));
}

#[tokio::test]
async fn remove_root_recursive_deletes_everything() {
    let mock = MockTransport::new();
    let (mut client, tree) = setup_simple(&mock).await;

    let mut resolver = MockNameResolver::new(&[]);
    let resolved = tree.resolve(&mut resolver, ROOT_URI).await.unwrap();

    // Post-order: beach.jpg (doc), Photos (dir), notes.txt (doc), then root itself
    mock.enqueue(delete_ok()); // beach.jpg
    mock.enqueue(delete_ok()); // Photos
    mock.enqueue(delete_ok()); // notes.txt
    mock.enqueue(delete_ok()); // root "self"

    let result = remove(&mut client, &tree, &resolved, true, "2026-03-01T12:00:00Z")
        .await
        .unwrap();

    assert_eq!(result.documents_deleted, 2); // beach.jpg + notes.txt
    assert_eq!(result.directories_deleted, 2); // Photos + root
}
