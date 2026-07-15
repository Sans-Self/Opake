use std::collections::HashMap;

use super::*;
use crate::client::HttpResponse;
use crate::test_utils::MockTransport;

use super::super::tests::{
    dummy_directory_with_entries, list_records_response, mock_client, test_keypair, TEST_DID,
};

const ROOT_URI: &str = "at://did:plc:test/at.opake.directory/self";
const DIR_PHOTOS_URI: &str = "at://did:plc:test/at.opake.directory/photos";
const DIR_VACATION_URI: &str = "at://did:plc:test/at.opake.directory/vacation";
const DOC_BEACH_URI: &str = "at://did:plc:test/at.opake.document/beach";
const DOC_NOTES_URI: &str = "at://did:plc:test/at.opake.document/notes";
const DOC_SUNSET_URI: &str = "at://did:plc:test/at.opake.document/sunset";

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
    let kp = test_keypair();
    tree.decrypt_names(TEST_DID, &kp.private_keys());
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
    let kp = test_keypair();
    tree.decrypt_names(TEST_DID, &kp.private_keys());
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
        .resolve(&mut resolver, "at://did:plc:test/at.opake.grant/nope")
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

    let kp = test_keypair();
    tree.decrypt_names(TEST_DID, &kp.private_keys());
    let mut resolver = MockNameResolver::new(&[]);

    let resolved = tree.resolve(&mut resolver, "Photos").await.unwrap();
    assert_eq!(resolved.uri, "at://did:plc:test/at.opake.directory/photos");
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
                    vec!["at://did:plc:test/at.opake.directory/empty".into()],
                ),
            ),
            ("empty", dummy_directory_with_entries("Empty", vec![])),
        ],
        None,
    ));

    let mut client = mock_client(mock);
    let tree = DirectoryTree::load(&mut client).await.unwrap();

    let descendants = tree.collect_descendants("at://did:plc:test/at.opake.directory/empty");
    assert!(descendants.is_empty());
}

// -- from_records --

#[test]
fn from_records_detects_root() {
    let kp = test_keypair();
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
    tree.decrypt_names(TEST_DID, &kp.private_keys());

    assert_eq!(tree.root_uri(), Some(ROOT_URI));
    assert_eq!(tree.directory_name(ROOT_URI), Some("/"));
    assert_eq!(tree.directory_name(DIR_PHOTOS_URI), Some("Photos"));
}

#[test]
fn from_records_empty() {
    let tree = DirectoryTree::from_records(std::iter::empty());
    assert!(tree.root_uri().is_none());
}

/// Regression: the indexer snapshot returns the whole directory chain, so a
/// superseded predecessor of a child's parent is present alongside the head.
/// `find_parent` must return the *canonical* (chain-head) parent, never a
/// superseded one — otherwise walking up to the root lands on a stale root
/// that no longer matches the indexer's chain head (the symptom: editing a
/// document in a workspace with rename/supersede history fails with "not
/// reachable from indexer's root").
// spec:tree-chains § Consumers build the live tree from chain heads only
#[test]
#[allow(non_snake_case)] // bug__ regression-naming convention
fn bug__find_parent_skips_superseded_parent() {
    const DOC_URI: &str = "at://did:plc:test/at.opake.document/doc1";
    const PARENT_OLD: &str = "at://did:plc:test/at.opake.directory/parent_old";
    const PARENT_NEW: &str = "at://did:plc:test/at.opake.directory/parent_new";

    // Both versions of the parent list the doc; PARENT_NEW supersedes
    // PARENT_OLD.
    let parent_new = {
        let mut d = dummy_directory_with_entries("Folder", vec![DOC_URI.into()]);
        d.supersedes = Some(PARENT_OLD.into());
        d
    };

    let records = vec![
        (
            PARENT_OLD.to_owned(),
            dummy_directory_with_entries("Folder", vec![DOC_URI.into()]),
        ),
        (PARENT_NEW.to_owned(), parent_new),
    ];

    let tree = DirectoryTree::from_records(records);

    assert_eq!(
        tree.find_parent(DOC_URI).as_deref(),
        Some(PARENT_NEW),
        "find_parent must return the chain-head parent, not the superseded one"
    );
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

// ---------------------------------------------------------------------------
// decrypt_names_with_group_keys
// ---------------------------------------------------------------------------

use crate::crypto::{self as crypto_mod, ContentKey};
// KeyWrapping types used by the keyring directory test helper (via encrypt_keyring_directory_envelope)

/// Build a keyring-encrypted directory for testing.
fn keyring_directory(
    name: &str,
    keyring_uri: &str,
    group_key: &ContentKey,
    entries: Vec<String>,
) -> Directory {
    let (encryption, encrypted_metadata) = crate::directories::encrypt_keyring_directory_envelope(
        name,
        None,
        keyring_uri,
        group_key,
        0,
        &mut crypto_mod::OsRng,
    )
    .unwrap();

    let entries = entries
        .into_iter()
        .map(|target| crate::records::ListingEntry::new(target, "bafytest"))
        .collect();
    Directory {
        entries,
        ..Directory::new(
            encryption,
            encrypted_metadata,
            "2026-03-21T00:00:00Z".into(),
        )
    }
}

#[test]
fn decrypt_names_with_group_keys_decrypts_keyring_directories() {
    let group_key = crypto_mod::generate_content_key(&mut crypto_mod::OsRng);
    let keyring_uri = "at://did:plc:test/at.opake.keyring/kr1";

    let root = keyring_directory(
        "/",
        keyring_uri,
        &group_key,
        vec!["at://did:plc:test/at.opake.directory/sub1".into()],
    );
    let sub = keyring_directory("Projects", keyring_uri, &group_key, vec![]);

    let records = vec![
        ("at://did:plc:test/at.opake.directory/wsroot".into(), root),
        ("at://did:plc:test/at.opake.directory/sub1".into(), sub),
    ];

    let mut tree = DirectoryTree::from_records(records);
    let kp = test_keypair();

    let historical: Vec<crate::workspace::HistoricalKey> = Vec::new();
    let view = crate::workspace::GroupKeys {
        current_rotation: 0,
        current: &group_key,
        historical: &historical,
    };
    let mut group_keys = HashMap::new();
    group_keys.insert(keyring_uri.to_string(), view);

    tree.decrypt_names_with_group_keys(TEST_DID, &kp.private_keys(), &group_keys);

    assert_eq!(
        tree.directory_name("at://did:plc:test/at.opake.directory/sub1"),
        Some("Projects")
    );
}

#[test]
fn decrypt_names_with_group_keys_handles_mixed_encryption() {
    let group_key = crypto_mod::generate_content_key(&mut crypto_mod::OsRng);
    let keyring_uri = "at://did:plc:test/at.opake.keyring/kr1";

    // One keyring-encrypted directory
    let keyring_dir = keyring_directory("Workspace", keyring_uri, &group_key, vec![]);

    // One direct-encrypted directory (from test helpers)
    let direct_dir = super::super::tests::dummy_directory("Personal");

    let records = vec![
        (
            "at://did:plc:test/at.opake.directory/ws".into(),
            keyring_dir,
        ),
        (
            "at://did:plc:test/at.opake.directory/personal".into(),
            direct_dir,
        ),
    ];

    let mut tree = DirectoryTree::from_records(records);
    let kp = test_keypair();

    let historical: Vec<crate::workspace::HistoricalKey> = Vec::new();
    let view = crate::workspace::GroupKeys {
        current_rotation: 0,
        current: &group_key,
        historical: &historical,
    };
    let mut group_keys = HashMap::new();
    group_keys.insert(keyring_uri.to_string(), view);

    tree.decrypt_names_with_group_keys(TEST_DID, &kp.private_keys(), &group_keys);

    assert_eq!(
        tree.directory_name("at://did:plc:test/at.opake.directory/ws"),
        Some("Workspace")
    );
    assert_eq!(
        tree.directory_name("at://did:plc:test/at.opake.directory/personal"),
        Some("Personal")
    );
}

#[test]
fn decrypt_names_with_group_keys_falls_back_for_unknown_keyring() {
    let group_key = crypto_mod::generate_content_key(&mut crypto_mod::OsRng);
    let keyring_uri = "at://did:plc:test/at.opake.keyring/kr1";

    let dir = keyring_directory("Secret", keyring_uri, &group_key, vec![]);

    let records = vec![("at://did:plc:test/at.opake.directory/secret".into(), dir)];

    let mut tree = DirectoryTree::from_records(records);
    let kp = test_keypair();

    // Empty group keys map — keyring not known
    let group_keys = HashMap::new();
    tree.decrypt_names_with_group_keys(TEST_DID, &kp.private_keys(), &group_keys);

    assert_eq!(
        tree.directory_name("at://did:plc:test/at.opake.directory/secret"),
        Some("?")
    );
}

// ---------------------------------------------------------------------------
// Incremental mutation via apply_directory_delta (SSE path)
// ---------------------------------------------------------------------------

fn cabinet_ctx<'a>(private_keys: &'a crate::crypto::PrivateKeyBundle<'a>) -> DecryptionCtx<'a> {
    DecryptionCtx::cabinet(TEST_DID, private_keys)
}

#[test]
fn apply_directory_delta_inserts_new_directory() {
    let mut tree = DirectoryTree::from_records(std::iter::empty());
    let kp = test_keypair();

    let photos_dir = dummy_directory_with_entries("Photos", vec![DOC_BEACH_URI.into()]);

    let change = tree
        .apply_directory_delta(
            DIR_PHOTOS_URI,
            &photos_dir,
            &cabinet_ctx(&kp.private_keys()),
        )
        .unwrap();

    match change {
        TreeChange::Inserted { uri } => assert_eq!(uri, DIR_PHOTOS_URI),
        other => panic!("expected Inserted, got {other:?}"),
    }

    assert_eq!(tree.directory_name(DIR_PHOTOS_URI), Some("Photos"));
    assert_eq!(
        tree.entries_for(DIR_PHOTOS_URI),
        Some(&[DOC_BEACH_URI.to_string()][..])
    );
}

#[test]
fn apply_directory_delta_detects_root_by_self_rkey() {
    let mut tree = DirectoryTree::from_records(std::iter::empty());
    let kp = test_keypair();

    let root_dir = dummy_directory_with_entries("/", vec![]);

    let change = tree
        .apply_directory_delta(ROOT_URI, &root_dir, &cabinet_ctx(&kp.private_keys()))
        .unwrap();
    assert!(matches!(change, TreeChange::Inserted { .. }));

    assert_eq!(tree.root_uri(), Some(ROOT_URI));
    assert_eq!(tree.directory_name(ROOT_URI), Some("/"));
}

#[test]
fn apply_directory_delta_updates_existing_directory() {
    let mut tree = DirectoryTree::from_records(std::iter::empty());
    let kp = test_keypair();

    let initial = dummy_directory_with_entries("Photos", vec![DOC_BEACH_URI.into()]);
    let change = tree
        .apply_directory_delta(DIR_PHOTOS_URI, &initial, &cabinet_ctx(&kp.private_keys()))
        .unwrap();
    assert!(matches!(change, TreeChange::Inserted { .. }));

    let updated =
        dummy_directory_with_entries("Photos", vec![DOC_BEACH_URI.into(), DOC_NOTES_URI.into()]);
    let change = tree
        .apply_directory_delta(DIR_PHOTOS_URI, &updated, &cabinet_ctx(&kp.private_keys()))
        .unwrap();

    match change {
        TreeChange::Updated { uri } => assert_eq!(uri, DIR_PHOTOS_URI),
        other => panic!("expected Updated, got {other:?}"),
    }

    assert_eq!(
        tree.entries_for(DIR_PHOTOS_URI),
        Some(&[DOC_BEACH_URI.to_string(), DOC_NOTES_URI.to_string()][..])
    );
}

#[test]
fn apply_directory_delete_removes_directory() {
    let mut tree = DirectoryTree::from_records(std::iter::empty());
    let kp = test_keypair();

    let photos = dummy_directory_with_entries("Photos", vec![]);
    tree.apply_directory_delta(DIR_PHOTOS_URI, &photos, &cabinet_ctx(&kp.private_keys()))
        .unwrap();
    assert!(tree.is_directory(DIR_PHOTOS_URI));

    let change = tree.apply_directory_delete(DIR_PHOTOS_URI);

    match change {
        TreeChange::Removed { uri } => assert_eq!(uri, DIR_PHOTOS_URI),
        other => panic!("expected Removed, got {other:?}"),
    }

    assert!(!tree.is_directory(DIR_PHOTOS_URI));
}

#[test]
fn apply_directory_delete_on_missing_is_noop() {
    let mut tree = DirectoryTree::from_records(std::iter::empty());

    let change = tree.apply_directory_delete(DIR_PHOTOS_URI);

    assert_eq!(change, TreeChange::NoOp);
    assert!(!change.is_effective());
}

#[test]
fn apply_directory_delete_root_clears_root_uri() {
    let mut tree = DirectoryTree::from_records(std::iter::empty());
    let kp = test_keypair();

    let root_dir = dummy_directory_with_entries("/", vec![]);
    tree.apply_directory_delta(ROOT_URI, &root_dir, &cabinet_ctx(&kp.private_keys()))
        .unwrap();
    assert_eq!(tree.root_uri(), Some(ROOT_URI));

    tree.apply_directory_delete(ROOT_URI);
    assert_eq!(tree.root_uri(), None);
}

#[test]
fn apply_directory_delta_idempotent_repeat_apply_preserves_state() {
    let mut tree = DirectoryTree::from_records(std::iter::empty());
    let kp = test_keypair();

    let dir = dummy_directory_with_entries("Photos", vec![DOC_BEACH_URI.into()]);

    // First apply — inserts.
    let c1 = tree
        .apply_directory_delta(DIR_PHOTOS_URI, &dir, &cabinet_ctx(&kp.private_keys()))
        .unwrap();
    assert!(matches!(c1, TreeChange::Inserted { .. }));

    // Second apply — counts as Updated (the tree layer doesn't dedupe).
    let c2 = tree
        .apply_directory_delta(DIR_PHOTOS_URI, &dir, &cabinet_ctx(&kp.private_keys()))
        .unwrap();
    assert!(matches!(c2, TreeChange::Updated { .. }));

    assert_eq!(tree.directory_name(DIR_PHOTOS_URI), Some("Photos"));
    assert_eq!(
        tree.entries_for(DIR_PHOTOS_URI),
        Some(&[DOC_BEACH_URI.to_string()][..])
    );
}

#[test]
fn apply_directory_delta_falls_back_to_question_mark_on_missing_key() {
    let mut tree = DirectoryTree::from_records(std::iter::empty());
    let kp = test_keypair();

    let dir = dummy_directory_with_entries("Photos", vec![]);

    let private_keys = kp.private_keys();
    let wrong_ctx = DecryptionCtx::cabinet("did:plc:wrong", &private_keys);
    let change = tree
        .apply_directory_delta(DIR_PHOTOS_URI, &dir, &wrong_ctx)
        .unwrap();
    assert!(matches!(change, TreeChange::Inserted { .. }));
    assert_eq!(tree.directory_name(DIR_PHOTOS_URI), Some("?"));
}

#[test]
fn invalidate_decrypted_names_clears_all_names() {
    let mut tree = DirectoryTree::from_records(std::iter::empty());
    let kp = test_keypair();

    let dir = dummy_directory_with_entries("Photos", vec![]);
    tree.apply_directory_delta(DIR_PHOTOS_URI, &dir, &cabinet_ctx(&kp.private_keys()))
        .unwrap();
    assert_eq!(tree.directory_name(DIR_PHOTOS_URI), Some("Photos"));

    tree.invalidate_decrypted_names();
    assert_eq!(tree.directory_name(DIR_PHOTOS_URI), Some(""));
}

#[test]
fn tree_change_uri_and_is_effective() {
    let inserted = TreeChange::Inserted {
        uri: "at://a".into(),
    };
    assert_eq!(inserted.uri(), Some("at://a"));
    assert!(inserted.is_effective());

    let noop = TreeChange::NoOp;
    assert_eq!(noop.uri(), None);
    assert!(!noop.is_effective());
}

/// Defensive: a directory tree containing a cycle (A lists B, B lists A)
/// must not send `collect_descendants` into an unbounded walk. The
/// domain-API cycle guard keeps honest clients from ever writing such a
/// tree, but a hostile or buggy writer can, and every tree consumer
/// should terminate on it rather than hang.
///
/// Matching the tree-walking guards elsewhere (`is_reachable_from_root`,
/// `reject_cycle`), the walk is bounded rather than fallible: it yields the
/// reachable set and returns. Erroring is reserved for supersede-chain
/// walks, where a cycle means the record set itself is invalid.
#[test]
#[allow(non_snake_case)] // bug__ regression-naming convention
fn bug__collect_descendants_terminates_on_cyclic_tree() {
    use std::sync::mpsc;
    use std::time::Duration;

    let dir_a = "at://did:plc:test/at.opake.directory/cycleA".to_string();
    let dir_b = "at://did:plc:test/at.opake.directory/cycleB".to_string();
    let tree = DirectoryTree::from_records(vec![
        (
            dir_a.clone(),
            dummy_directory_with_entries("A", vec![dir_b.clone()]),
        ),
        (
            dir_b.clone(),
            dummy_directory_with_entries("B", vec![dir_a.clone()]),
        ),
    ]);

    let (tx, rx) = mpsc::channel();
    let expected_b = dir_b.clone();
    std::thread::spawn(move || {
        let _ = tx.send(tree.collect_descendants(&dir_a));
    });

    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(descendants) => assert_eq!(
            descendants,
            vec![(expected_b, EntryKind::Directory)],
            "cycle member should be collected once, the cycle back-edge dropped"
        ),
        Err(_) => panic!("collect_descendants did not terminate on cyclic input within 2s"),
    }
}

// ---------------------------------------------------------------------------
// Placeholder rendering for corrupt containers (poison-record-resilience 3.1)
// ---------------------------------------------------------------------------

const PH_ROOT: &str = "at://did:plc:test/at.opake.directory/self";
const PH_CHILD: &str = "at://did:plc:test/at.opake.directory/corrupt-child";

fn root_listing(children: Vec<String>) -> DirectoryTree {
    DirectoryTree::from_records(vec![(
        PH_ROOT.to_string(),
        dummy_directory_with_entries("/", children),
    )])
}

#[test]
fn corrupt_referenced_directory_becomes_placeholder() {
    let mut tree = root_listing(vec![PH_CHILD.into()]);
    let tally = tree.apply_unreadable_refs(&[crate::records::UnreadableRef::corrupt(Some(
        PH_CHILD.into(),
    ))]);

    assert_eq!(tally.placeholders, 1);
    assert_eq!(tally.count_only, 0);
    assert!(tree.is_placeholder(PH_CHILD));
    assert!(tree.is_directory(PH_CHILD));
    // Name is client-assigned, never record content.
    assert_eq!(
        tree.directory_name(PH_CHILD),
        Some(PLACEHOLDER_DISPLAY_NAME)
    );
    assert_eq!(
        tree.placeholder_reason(PH_CHILD),
        Some(crate::records::UnreadableReason::Corrupt)
    );
}

#[test]
fn unreferenced_corrupt_directory_is_count_only() {
    // Root references nothing → no authorized reference → no placeholder,
    // no URI disclosure into the tree.
    let mut tree = root_listing(vec![]);
    let tally = tree.apply_unreadable_refs(&[crate::records::UnreadableRef::corrupt(Some(
        PH_CHILD.into(),
    ))]);

    assert_eq!(tally.placeholders, 0);
    assert_eq!(tally.count_only, 1);
    assert!(!tree.is_placeholder(PH_CHILD));
}

#[test]
fn uriless_corrupt_ref_is_count_only() {
    let mut tree = root_listing(vec![PH_CHILD.into()]);
    let tally = tree.apply_unreadable_refs(&[crate::records::UnreadableRef::corrupt(None)]);
    assert_eq!(tally.count_only, 1);
    assert_eq!(tally.placeholders, 0);
}

#[test]
fn corrupt_document_ref_is_count_only() {
    const DOC: &str = "at://did:plc:test/at.opake.document/corrupt-doc";
    let mut tree = root_listing(vec![DOC.into()]);
    // A corrupt document has no children and no node shape → count-only.
    let tally =
        tree.apply_unreadable_refs(&[crate::records::UnreadableRef::corrupt(Some(DOC.into()))]);
    assert_eq!(tally.count_only, 1);
    assert_eq!(tally.placeholders, 0);
    assert!(!tree.is_placeholder(DOC));
}

#[test]
fn future_version_referenced_directory_is_placeholder_with_reason() {
    let mut tree = root_listing(vec![PH_CHILD.into()]);
    let tally = tree.apply_unreadable_refs(&[crate::records::UnreadableRef::needs_newer_client(
        Some(PH_CHILD.into()),
    )]);
    assert_eq!(tally.placeholders, 1);
    assert_eq!(
        tree.placeholder_reason(PH_CHILD),
        Some(crate::records::UnreadableReason::NeedsNewerClient)
    );
}

#[test]
fn readable_record_upgrades_a_placeholder() {
    let mut tree = root_listing(vec![PH_CHILD.into()]);
    tree.apply_unreadable_refs(&[crate::records::UnreadableRef::corrupt(Some(
        PH_CHILD.into(),
    ))]);
    assert!(tree.is_placeholder(PH_CHILD));

    // A readable record for the same URI supersedes the placeholder.
    let kp = test_keypair();
    let good = dummy_directory_with_entries("Recovered", vec![]);
    tree.apply_directory_delta(PH_CHILD, &good, &cabinet_ctx(&kp.private_keys()))
        .unwrap();

    assert!(!tree.is_placeholder(PH_CHILD));
    assert!(tree.is_directory(PH_CHILD));
}

#[test]
fn placeholder_position_is_stable_across_reapplication() {
    let mut tree = root_listing(vec![PH_CHILD.into()]);
    tree.apply_unreadable_refs(&[crate::records::UnreadableRef::corrupt(Some(
        PH_CHILD.into(),
    ))]);
    let before: Vec<String> = tree.entries_for(PH_ROOT).unwrap().to_vec();

    // Re-applying the same ref (e.g. a snapshot refresh) is idempotent and
    // never moves the placeholder: its position follows the surviving
    // reference from the root, which did not change.
    tree.apply_unreadable_refs(&[crate::records::UnreadableRef::corrupt(Some(
        PH_CHILD.into(),
    ))]);
    assert!(tree.is_placeholder(PH_CHILD));
    assert_eq!(tree.entries_for(PH_ROOT).unwrap().to_vec(), before);
}

#[test]
fn degraded_directory_keeps_children_beneath_placeholder() {
    const GRANDCHILD: &str = "at://did:plc:test/at.opake.directory/gc";
    // A readable child directory with its own child, referenced by root.
    let kp = test_keypair();
    let mut tree = DirectoryTree::from_records(vec![
        (
            PH_ROOT.to_string(),
            dummy_directory_with_entries("/", vec![PH_CHILD.into()]),
        ),
        (
            PH_CHILD.to_string(),
            dummy_directory_with_entries("Child", vec![GRANDCHILD.into()]),
        ),
    ]);
    tree.decrypt_names(TEST_DID, &kp.private_keys());

    // The child is corrupted (e.g. a poison supersede). Degrading it preserves
    // its children so the subtree stays attached beneath the placeholder.
    tree.apply_unreadable_refs(&[crate::records::UnreadableRef::corrupt(Some(
        PH_CHILD.into(),
    ))]);

    assert!(tree.is_placeholder(PH_CHILD));
    assert_eq!(
        tree.directory_name(PH_CHILD),
        Some(PLACEHOLDER_DISPLAY_NAME)
    );
    assert_eq!(
        tree.entries_for(PH_CHILD).map(|e| e.to_vec()),
        Some(vec![GRANDCHILD.to_string()]),
        "children remain attached and visible beneath the placeholder"
    );
}
