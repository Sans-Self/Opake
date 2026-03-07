use super::*;
use crate::client::HttpResponse;
use crate::test_utils::MockTransport;

use super::super::tests::{
    dummy_directory_with_entries, list_records_response, mock_client, test_keypair, TEST_DID,
};

const ROOT_URI: &str = "at://did:plc:test/app.opake.directory/self";
const DIR_PHOTOS_URI: &str = "at://did:plc:test/app.opake.directory/photos";
const DIR_VACATION_URI: &str = "at://did:plc:test/app.opake.directory/vacation";
const DOC_BEACH_URI: &str = "at://did:plc:test/app.opake.document/beach";
const DOC_NOTES_URI: &str = "at://did:plc:test/app.opake.document/notes";
const DOC_SUNSET_URI: &str = "at://did:plc:test/app.opake.document/sunset";

/// getRecord response for a document — minimal but parseable.
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
///
/// Only enqueues the directory listing. Document getRecord calls are
/// enqueued by individual tests as needed for resolve.
async fn load_simple_tree(mock: &MockTransport) -> DirectoryTree {
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
    tree
}

/// Load a nested tree: / → Photos → [Vacation → [sunset.jpg], beach.jpg]
async fn load_nested_tree(mock: &MockTransport) -> DirectoryTree {
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
    tree
}

// -- load --

#[tokio::test]
async fn load_with_no_root() {
    let mock = MockTransport::new();
    mock.enqueue(list_records_response(&[], None));

    let mut client = mock_client(mock);
    let tree = DirectoryTree::load(&mut client).await.unwrap();
    assert!(tree.root_uri.is_none());
}

#[tokio::test]
async fn load_propagates_pds_error() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 500,
        headers: vec![],
        body: br#"{"error":"InternalServerError","message":"boom"}"#.to_vec(),
    });

    let mut client = mock_client(mock);
    let err = DirectoryTree::load(&mut client).await.unwrap_err();
    assert!(matches!(err, Error::Xrpc { .. }));
}

#[tokio::test]
async fn load_detects_root_from_listing() {
    let mock = MockTransport::new();
    mock.enqueue(list_records_response(
        &[("self", dummy_directory_with_entries("/", vec![]))],
        None,
    ));

    let mut client = mock_client(mock);
    let tree = DirectoryTree::load(&mut client).await.unwrap();
    assert_eq!(tree.root_uri.as_deref(), Some(ROOT_URI));
}

// -- resolve: AT-URI --

#[tokio::test]
async fn resolve_at_uri_directory() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;

    let mut client = mock_client(mock);
    let resolved = tree.resolve(&mut client, DIR_PHOTOS_URI).await.unwrap();
    assert_eq!(resolved.kind, EntryKind::Directory);
    assert_eq!(resolved.name, "Photos");
    assert_eq!(resolved.parent_uri.as_deref(), Some(ROOT_URI));
}

#[tokio::test]
async fn resolve_at_uri_document() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;

    // getRecord for the document to fetch its name
    mock.enqueue(doc_record_response(DOC_BEACH_URI, "beach.jpg"));

    let mut client = mock_client(mock);
    let resolved = tree.resolve(&mut client, DOC_BEACH_URI).await.unwrap();
    assert_eq!(resolved.uri, DOC_BEACH_URI);
    assert_eq!(resolved.kind, EntryKind::Document);
    assert_eq!(resolved.name, "beach.jpg");
    assert_eq!(resolved.parent_uri.as_deref(), Some(DIR_PHOTOS_URI));
}

#[tokio::test]
async fn resolve_at_uri_not_found() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;

    // getRecord 404 for unknown document
    mock.enqueue(HttpResponse {
        status: 404,
        headers: vec![],
        body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
    });

    let mut client = mock_client(mock);
    let err = tree
        .resolve(&mut client, "at://did:plc:test/app.opake.document/nope")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

// -- resolve: path --

#[tokio::test]
async fn resolve_path_document_in_subdirectory() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;

    // getRecord for beach.jpg (document child of Photos)
    mock.enqueue(doc_record_response(DOC_BEACH_URI, "beach.jpg"));

    let mut client = mock_client(mock);
    let resolved = tree.resolve(&mut client, "Photos/beach.jpg").await.unwrap();
    assert_eq!(resolved.uri, DOC_BEACH_URI);
    assert_eq!(resolved.kind, EntryKind::Document);
    assert_eq!(resolved.parent_uri.as_deref(), Some(DIR_PHOTOS_URI));
}

#[tokio::test]
async fn resolve_path_nested() {
    let mock = MockTransport::new();
    let tree = load_nested_tree(&mock).await;

    // getRecord for sunset.jpg (document child of Vacation)
    mock.enqueue(doc_record_response(DOC_SUNSET_URI, "sunset.jpg"));

    let mut client = mock_client(mock);
    let resolved = tree
        .resolve(&mut client, "Photos/Vacation/sunset.jpg")
        .await
        .unwrap();
    assert_eq!(resolved.uri, DOC_SUNSET_URI);
    assert_eq!(resolved.kind, EntryKind::Document);
}

#[tokio::test]
async fn resolve_path_directory_target() {
    let mock = MockTransport::new();
    let tree = load_nested_tree(&mock).await;

    // Vacation found in memory, but find_child_any still scans document
    // children of Photos for ambiguity.
    mock.enqueue(doc_record_response(DOC_BEACH_URI, "beach.jpg"));

    let mut client = mock_client(mock);
    let resolved = tree.resolve(&mut client, "Photos/Vacation").await.unwrap();
    assert_eq!(resolved.uri, DIR_VACATION_URI);
    assert_eq!(resolved.kind, EntryKind::Directory);
}

#[tokio::test]
async fn resolve_path_not_found_segment() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;

    // getRecord for the one document child of Photos — no match
    mock.enqueue(doc_record_response(DOC_BEACH_URI, "beach.jpg"));

    let mut client = mock_client(mock);
    let err = tree
        .resolve(&mut client, "Photos/missing.txt")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

#[tokio::test]
async fn resolve_path_missing_intermediate_directory() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;

    let mut client = mock_client(mock);
    let err = tree
        .resolve(&mut client, "Nope/beach.jpg")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

#[tokio::test]
async fn resolve_path_no_root_errors() {
    let mock = MockTransport::new();
    mock.enqueue(list_records_response(&[], None));

    let mut client = mock_client(mock.clone());
    let tree = DirectoryTree::load(&mut client).await.unwrap();

    let mut client = mock_client(mock);
    let err = tree
        .resolve(&mut client, "Photos/beach.jpg")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

// -- resolve: bare name --

#[tokio::test]
async fn resolve_bare_name_document() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;

    // Root has 2 entries: DIR_PHOTOS_URI (directory, checked in memory)
    // and DOC_NOTES_URI (document, needs getRecord).
    mock.enqueue(doc_record_response(DOC_NOTES_URI, "notes.txt"));

    let mut client = mock_client(mock);
    let resolved = tree.resolve(&mut client, "notes.txt").await.unwrap();
    assert_eq!(resolved.uri, DOC_NOTES_URI);
    assert_eq!(resolved.kind, EntryKind::Document);
}

#[tokio::test]
async fn resolve_bare_name_directory() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;

    // Photos is a directory — found in memory, no document getRecords needed
    // because directory match is found first.
    // But find_child_any still scans document children for ambiguity.
    mock.enqueue(doc_record_response(DOC_NOTES_URI, "notes.txt"));

    let mut client = mock_client(mock);
    let resolved = tree.resolve(&mut client, "Photos").await.unwrap();
    assert_eq!(resolved.uri, DIR_PHOTOS_URI);
    assert_eq!(resolved.kind, EntryKind::Directory);
}

#[tokio::test]
async fn resolve_bare_name_not_found() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;

    // Scans root's document children (notes.txt) — no match.
    mock.enqueue(doc_record_response(DOC_NOTES_URI, "notes.txt"));

    let mut client = mock_client(mock);
    let err = tree.resolve(&mut client, "missing.txt").await.unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

#[tokio::test]
async fn resolve_bare_name_no_root_searches_directories() {
    let mock = MockTransport::new();
    // No root, but a directory named "Photos" exists.
    mock.enqueue(list_records_response(
        &[("photos", dummy_directory_with_entries("Photos", vec![]))],
        None,
    ));

    let mut client = mock_client(mock.clone());
    let mut tree = DirectoryTree::load(&mut client).await.unwrap();
    assert!(tree.root_uri.is_none());

    let (_, private_key) = test_keypair();
    tree.decrypt_names(TEST_DID, &private_key);

    let mut client = mock_client(mock);
    let resolved = tree.resolve(&mut client, "Photos").await.unwrap();
    assert_eq!(resolved.uri, "at://did:plc:test/app.opake.directory/photos");
    assert_eq!(resolved.kind, EntryKind::Directory);
}

// -- count_descendants --

#[tokio::test]
async fn count_descendants_flat() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;
    let (docs, dirs) = tree.count_descendants(ROOT_URI);
    assert_eq!(docs, 2); // notes.txt (root) + beach.jpg (via Photos)
    assert_eq!(dirs, 1); // Photos
}

#[tokio::test]
async fn count_descendants_nested() {
    let mock = MockTransport::new();
    let tree = load_nested_tree(&mock).await;
    let (docs, dirs) = tree.count_descendants(DIR_PHOTOS_URI);
    assert_eq!(docs, 2); // beach.jpg + sunset.jpg (via Vacation)
    assert_eq!(dirs, 1); // Vacation
}

#[tokio::test]
async fn count_descendants_empty_directory() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;
    let (docs, dirs) = tree.count_descendants(DIR_PHOTOS_URI);
    assert_eq!(docs, 1); // beach.jpg
    assert_eq!(dirs, 0);
}

// -- collect_descendants --

#[tokio::test]
async fn collect_descendants_post_order() {
    let mock = MockTransport::new();
    let tree = load_nested_tree(&mock).await;
    let descendants = tree.collect_descendants(DIR_PHOTOS_URI);

    let uris: Vec<&str> = descendants.iter().map(|(uri, _)| uri.as_str()).collect();

    // sunset.jpg must come before Vacation (post-order)
    let sunset_pos = uris.iter().position(|u| *u == DOC_SUNSET_URI).unwrap();
    let vacation_pos = uris.iter().position(|u| *u == DIR_VACATION_URI).unwrap();
    assert!(sunset_pos < vacation_pos, "children must precede parents");

    assert_eq!(descendants.len(), 3); // sunset, vacation, beach
}

#[tokio::test]
async fn collect_descendants_empty() {
    let mock = MockTransport::new();
    mock.enqueue(list_records_response(
        &[
            (
                "self",
                dummy_directory_with_entries(
                    "/",
                    vec!["at://did:plc:test/app.opake.directory/empty".into()],
                ),
            ),
            ("empty", dummy_directory_with_entries("Empty", vec![])),
        ],
        None,
    ));

    let mut client = mock_client(mock);
    let tree = DirectoryTree::load(&mut client).await.unwrap();

    let descendants = tree.collect_descendants("at://did:plc:test/app.opake.directory/empty");
    assert!(descendants.is_empty());
}
