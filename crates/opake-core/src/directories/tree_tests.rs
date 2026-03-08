use std::collections::HashMap;

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

/// Load a simple tree: / → [Photos → [beach.jpg], notes.txt]
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
    let mut resolver = MockNameResolver::new(&[]);

    let resolved = tree.resolve(&mut resolver, DIR_PHOTOS_URI).await.unwrap();
    assert_eq!(resolved.kind, EntryKind::Directory);
    assert_eq!(resolved.name, "Photos");
    assert_eq!(resolved.parent_uri.as_deref(), Some(ROOT_URI));
}

#[tokio::test]
async fn resolve_at_uri_document() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;
    let mut resolver = MockNameResolver::new(&[]);

    // AT-URI resolution uses rkey as name — no resolver call.
    let resolved = tree.resolve(&mut resolver, DOC_BEACH_URI).await.unwrap();
    assert_eq!(resolved.uri, DOC_BEACH_URI);
    assert_eq!(resolved.kind, EntryKind::Document);
    assert_eq!(resolved.name, "beach");
    assert_eq!(resolved.parent_uri.as_deref(), Some(DIR_PHOTOS_URI));
}

#[tokio::test]
async fn resolve_at_uri_unknown_collection() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;
    let mut resolver = MockNameResolver::new(&[]);

    let err = tree
        .resolve(&mut resolver, "at://did:plc:test/app.opake.grant/nope")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

// -- resolve: path --

#[tokio::test]
async fn resolve_path_document_in_subdirectory() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;
    let mut resolver = MockNameResolver::new(&[(DOC_BEACH_URI, "beach.jpg")]);

    let resolved = tree
        .resolve(&mut resolver, "Photos/beach.jpg")
        .await
        .unwrap();
    assert_eq!(resolved.uri, DOC_BEACH_URI);
    assert_eq!(resolved.kind, EntryKind::Document);
    assert_eq!(resolved.parent_uri.as_deref(), Some(DIR_PHOTOS_URI));
}

#[tokio::test]
async fn resolve_path_nested() {
    let mock = MockTransport::new();
    let tree = load_nested_tree(&mock).await;
    let mut resolver = MockNameResolver::new(&[(DOC_SUNSET_URI, "sunset.jpg")]);

    let resolved = tree
        .resolve(&mut resolver, "Photos/Vacation/sunset.jpg")
        .await
        .unwrap();
    assert_eq!(resolved.uri, DOC_SUNSET_URI);
    assert_eq!(resolved.kind, EntryKind::Document);
}

#[tokio::test]
async fn resolve_path_directory_target() {
    let mock = MockTransport::new();
    let tree = load_nested_tree(&mock).await;
    let mut resolver = MockNameResolver::new(&[(DOC_BEACH_URI, "beach.jpg")]);

    let resolved = tree
        .resolve(&mut resolver, "Photos/Vacation")
        .await
        .unwrap();
    assert_eq!(resolved.uri, DIR_VACATION_URI);
    assert_eq!(resolved.kind, EntryKind::Directory);
}

#[tokio::test]
async fn resolve_path_not_found_segment() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;
    let mut resolver = MockNameResolver::new(&[(DOC_BEACH_URI, "beach.jpg")]);

    let err = tree
        .resolve(&mut resolver, "Photos/missing.txt")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

#[tokio::test]
async fn resolve_path_missing_intermediate_directory() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;
    let mut resolver = MockNameResolver::new(&[]);

    let err = tree
        .resolve(&mut resolver, "Nope/beach.jpg")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

#[tokio::test]
async fn resolve_path_no_root_errors() {
    let mock = MockTransport::new();
    mock.enqueue(list_records_response(&[], None));

    let mut client = mock_client(mock);
    let tree = DirectoryTree::load(&mut client).await.unwrap();
    let mut resolver = MockNameResolver::new(&[]);

    let err = tree
        .resolve(&mut resolver, "Photos/beach.jpg")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

// -- resolve: bare name --

#[tokio::test]
async fn resolve_bare_name_document() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;
    let mut resolver = MockNameResolver::new(&[(DOC_NOTES_URI, "notes.txt")]);

    let resolved = tree.resolve(&mut resolver, "notes.txt").await.unwrap();
    assert_eq!(resolved.uri, DOC_NOTES_URI);
    assert_eq!(resolved.kind, EntryKind::Document);
}

#[tokio::test]
async fn resolve_bare_name_directory() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;
    let mut resolver = MockNameResolver::new(&[(DOC_NOTES_URI, "notes.txt")]);

    let resolved = tree.resolve(&mut resolver, "Photos").await.unwrap();
    assert_eq!(resolved.uri, DIR_PHOTOS_URI);
    assert_eq!(resolved.kind, EntryKind::Directory);
}

#[tokio::test]
async fn resolve_bare_name_not_found() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;
    let mut resolver = MockNameResolver::new(&[(DOC_NOTES_URI, "notes.txt")]);

    let err = tree
        .resolve(&mut resolver, "missing.txt")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

#[tokio::test]
async fn resolve_bare_name_no_root_searches_directories() {
    let mock = MockTransport::new();
    mock.enqueue(list_records_response(
        &[("photos", dummy_directory_with_entries("Photos", vec![]))],
        None,
    ));

    let mut client = mock_client(mock);
    let mut tree = DirectoryTree::load(&mut client).await.unwrap();
    assert!(tree.root_uri.is_none());

    let (_, private_key) = test_keypair();
    tree.decrypt_names(TEST_DID, &private_key);
    let mut resolver = MockNameResolver::new(&[]);

    let resolved = tree.resolve(&mut resolver, "Photos").await.unwrap();
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

// -- from_records --

#[test]
fn from_records_detects_root() {
    let (_, private_key) = test_keypair();
    let records = vec![
        (
            ROOT_URI.to_owned(),
            dummy_directory_with_entries("/", vec![DIR_PHOTOS_URI.into()]),
        ),
        (
            DIR_PHOTOS_URI.to_owned(),
            dummy_directory_with_entries("Photos", vec![]),
        ),
    ];

    let mut tree = DirectoryTree::from_records(records);
    tree.decrypt_names(TEST_DID, &private_key);

    assert_eq!(tree.root_uri(), Some(ROOT_URI));
    assert_eq!(tree.directory_name(ROOT_URI), Some("/"));
    assert_eq!(tree.directory_name(DIR_PHOTOS_URI), Some("Photos"));
}

#[test]
fn from_records_empty() {
    let tree = DirectoryTree::from_records(std::iter::empty());
    assert!(tree.root_uri().is_none());
}

// -- public getters --

#[tokio::test]
async fn getters_return_expected_values() {
    let mock = MockTransport::new();
    let tree = load_simple_tree(&mock).await;

    // root_uri
    assert_eq!(tree.root_uri(), Some(ROOT_URI));

    // entries_for
    let root_entries = tree.entries_for(ROOT_URI).unwrap();
    assert!(root_entries.contains(&DIR_PHOTOS_URI.to_owned()));
    assert!(root_entries.contains(&DOC_NOTES_URI.to_owned()));

    let photos_entries = tree.entries_for(DIR_PHOTOS_URI).unwrap();
    assert_eq!(photos_entries, &[DOC_BEACH_URI.to_owned()]);

    assert!(tree.entries_for("at://nonexistent").is_none());

    // directory_name
    assert_eq!(tree.directory_name(ROOT_URI), Some("/"));
    assert_eq!(tree.directory_name(DIR_PHOTOS_URI), Some("Photos"));
    assert!(tree.directory_name(DOC_BEACH_URI).is_none());

    // is_directory
    assert!(tree.is_directory(ROOT_URI));
    assert!(tree.is_directory(DIR_PHOTOS_URI));
    assert!(!tree.is_directory(DOC_BEACH_URI));

    // all_directory_uris
    let all_uris: Vec<&str> = tree.all_directory_uris().collect();
    assert_eq!(all_uris.len(), 2);
    assert!(all_uris.contains(&ROOT_URI));
    assert!(all_uris.contains(&DIR_PHOTOS_URI));

    // find_parent
    assert_eq!(tree.find_parent(DIR_PHOTOS_URI).as_deref(), Some(ROOT_URI));
    assert_eq!(tree.find_parent(DOC_NOTES_URI).as_deref(), Some(ROOT_URI));
    assert_eq!(
        tree.find_parent(DOC_BEACH_URI).as_deref(),
        Some(DIR_PHOTOS_URI)
    );
    assert!(tree.find_parent(ROOT_URI).is_none());
}
