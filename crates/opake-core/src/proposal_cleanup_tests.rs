use super::*;
use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
use crate::crypto::OsRng;
use crate::records::{
    AtBytes, BlobRef, CidLink, DirectoryUpdateRecord, DocumentUpdateRecord, EncryptedMetadata,
    KeyringUpdateRecord,
};
use crate::storage::{Identity, NoopStorage};
use crate::test_utils::MockTransport;

const DID: &str = "did:plc:editor";
const KEYRING_URI: &str = "at://did:plc:owner/app.opake.keyring/ws1";
const DOC_URI: &str = "at://did:plc:owner/app.opake.document/doc1";
const TARGET_DIR_URI: &str = "at://did:plc:owner/app.opake.directory/dir1";
const SOURCE_DIR_URI: &str = "at://did:plc:owner/app.opake.directory/source";

const EARLIER: &str = "2026-04-01T00:00:00Z";
const LATER: &str = "2026-04-02T00:00:00Z";

fn json_response<T: serde::Serialize>(value: &T) -> HttpResponse {
    HttpResponse {
        status: 200,
        body: serde_json::to_vec(value).unwrap(),
        headers: vec![],
    }
}

fn empty_records_page() -> HttpResponse {
    json_response(&serde_json::json!({ "records": [] }))
}

fn list_records_with(uri: &str, value: serde_json::Value) -> HttpResponse {
    json_response(&serde_json::json!({
        "records": [{ "uri": uri, "cid": "bafycid", "value": value }]
    }))
}

fn delete_response() -> HttpResponse {
    HttpResponse {
        status: 200,
        body: b"{}".to_vec(),
        headers: vec![],
    }
}

fn dummy_metadata() -> EncryptedMetadata {
    EncryptedMetadata {
        ciphertext: AtBytes {
            encoded: "AAAA".into(),
        },
        nonce: AtBytes {
            encoded: "BBBB".into(),
        },
    }
}

fn dummy_blob() -> BlobRef {
    BlobRef {
        blob_type: "blob".into(),
        reference: CidLink {
            cid: "bafytest".into(),
        },
        mime_type: "application/octet-stream".into(),
        size: 1,
    }
}

fn make_opake(mock: MockTransport) -> Opake<MockTransport, OsRng, NoopStorage> {
    let session = Session::Legacy(LegacySession {
        did: DID.into(),
        handle: "editor.test".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    let client = XrpcClient::with_session(mock, "https://editor-pds.test".into(), session);
    let identity = Identity::generate(DID, &mut OsRng);
    Opake::new(
        client,
        DID.into(),
        identity,
        OsRng,
        NoopStorage,
        || 1_700_000_000_000_000,
    )
    .unwrap()
}

/// SSE-driven cleanup: a single keyringUpdate.rename targeting the
/// known keyring; modifiedAt is later than createdAt → delete fires.
#[tokio::test]
async fn cleanup_for_target_deletes_when_modified_advances() {
    let proposal = serde_json::to_value(&KeyringUpdateRecord::rename(
        KEYRING_URI.into(),
        dummy_metadata(),
        EARLIER.into(),
    ))
    .unwrap();

    let mock = MockTransport::new();
    mock.enqueue(empty_records_page()); // documentUpdate
    mock.enqueue(empty_records_page()); // directoryUpdate
    mock.enqueue(list_records_with(
        "at://did:plc:editor/app.opake.keyringUpdate/abc",
        proposal,
    ));
    mock.enqueue(delete_response());

    let mut opake = make_opake(mock.clone());
    let deleted = opake
        .cleanup_proposals_for_target(KEYRING_URI, LATER)
        .await
        .unwrap();

    assert_eq!(deleted, 1, "stale proposal should be deleted");
    let requests = mock.requests();
    let delete_req = requests
        .iter()
        .find(|r| r.url.contains("deleteRecord"))
        .expect("expected a deleteRecord call");
    let body = match &delete_req.body {
        Some(crate::client::RequestBody::Json(v)) => v,
        _ => panic!("delete body wasn't JSON"),
    };
    assert_eq!(body["collection"], KEYRING_UPDATE_COLLECTION);
    assert_eq!(body["rkey"], "abc");
}

/// proposal.createdAt is *after* the modified_at signal — proposal is
/// fresher than the apply, leave it alone.
#[tokio::test]
async fn cleanup_for_target_keeps_proposal_when_modified_is_earlier() {
    let proposal = serde_json::to_value(&KeyringUpdateRecord::rename(
        KEYRING_URI.into(),
        dummy_metadata(),
        LATER.into(),
    ))
    .unwrap();

    let mock = MockTransport::new();
    mock.enqueue(empty_records_page());
    mock.enqueue(empty_records_page());
    mock.enqueue(list_records_with(
        "at://did:plc:editor/app.opake.keyringUpdate/abc",
        proposal,
    ));

    let mut opake = make_opake(mock.clone());
    let deleted = opake
        .cleanup_proposals_for_target(KEYRING_URI, EARLIER)
        .await
        .unwrap();

    assert_eq!(deleted, 0);
    assert!(
        !mock.requests().iter().any(|r| r.url.contains("deleteRecord")),
        "should not have called deleteRecord when proposal is newer"
    );
}

/// Cleanup ignores proposals targeting a different record.
#[tokio::test]
async fn cleanup_for_target_ignores_unrelated_proposals() {
    // Proposal targets a different document; cleanup is asked about DOC_URI.
    let proposal = serde_json::to_value(&DocumentUpdateRecord::update_content(
        "at://did:plc:owner/app.opake.document/other".into(),
        dummy_blob(),
        EARLIER.into(),
    ))
    .unwrap();

    let mock = MockTransport::new();
    mock.enqueue(list_records_with(
        "at://did:plc:editor/app.opake.documentUpdate/xyz",
        proposal,
    ));
    mock.enqueue(empty_records_page()); // directoryUpdate
    mock.enqueue(empty_records_page()); // keyringUpdate

    let mut opake = make_opake(mock.clone());
    let deleted = opake
        .cleanup_proposals_for_target(DOC_URI, LATER)
        .await
        .unwrap();

    assert_eq!(deleted, 0);
}

/// documentUpdate.updateContent matches by its document field.
#[tokio::test]
async fn cleanup_for_target_matches_update_content_via_document_uri() {
    let proposal = serde_json::to_value(&DocumentUpdateRecord::update_content(
        DOC_URI.into(),
        dummy_blob(),
        EARLIER.into(),
    ))
    .unwrap();

    let mock = MockTransport::new();
    mock.enqueue(list_records_with(
        "at://did:plc:editor/app.opake.documentUpdate/qrs",
        proposal,
    ));
    mock.enqueue(empty_records_page());
    mock.enqueue(empty_records_page());
    mock.enqueue(delete_response());

    let mut opake = make_opake(mock.clone());
    let deleted = opake
        .cleanup_proposals_for_target(DOC_URI, LATER)
        .await
        .unwrap();

    assert_eq!(deleted, 1);
}

/// directoryUpdate.moveEntry matches by its target_directory (not source).
#[tokio::test]
async fn cleanup_for_target_matches_move_entry_via_target_directory() {
    let proposal = serde_json::to_value(&DirectoryUpdateRecord::move_entry(
        KEYRING_URI.into(),
        SOURCE_DIR_URI.into(),
        TARGET_DIR_URI.into(),
        "at://did:plc:owner/app.opake.document/d1".into(),
        EARLIER.into(),
    ))
    .unwrap();

    let mock = MockTransport::new();
    mock.enqueue(empty_records_page()); // documentUpdate
    mock.enqueue(list_records_with(
        "at://did:plc:editor/app.opake.directoryUpdate/mov",
        proposal,
    ));
    mock.enqueue(empty_records_page()); // keyringUpdate
    mock.enqueue(delete_response());

    let mut opake = make_opake(mock.clone());
    let deleted = opake
        .cleanup_proposals_for_target(TARGET_DIR_URI, LATER)
        .await
        .unwrap();

    assert_eq!(deleted, 1);
}
