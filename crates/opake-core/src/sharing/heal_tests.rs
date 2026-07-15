// Healing is write-strict: it must never revoke a grant it cannot fully
// understand. The delete path (recipient gone) is exercised via e2e tests;
// these unit tests pin the poison-resilience guard.

use super::*;
use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
use crate::records::{AtBytes, Grant, WrappedKey, SCHEMA_VERSION};
use crate::test_utils::{dummy_encrypted_metadata, MockTransport};

const TEST_DID: &str = "did:plc:owner";

fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
    let session = Session::Legacy(LegacySession {
        did: TEST_DID.into(),
        handle: "owner.test".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    XrpcClient::with_session(mock, "https://pds.test".into(), session)
}

fn dummy_grant(recipient: &str) -> Grant {
    Grant::new(
        format!("at://{TEST_DID}/at.opake.document/doc1"),
        recipient.into(),
        WrappedKey {
            did: recipient.into(),
            ciphertext: AtBytes {
                encoded: "AAAA".into(),
            },
            algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
        },
        dummy_encrypted_metadata(),
        "2026-03-01T12:00:00Z".into(),
    )
}

fn list_grants_response(grant_value: serde_json::Value) -> HttpResponse {
    let body = serde_json::json!({
        "records": [{
            "uri": format!("at://{TEST_DID}/at.opake.grant/g1"),
            "cid": "bafygrant",
            "value": grant_value,
        }]
    });
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&body).unwrap(),
    }
}

// record-validity § writes refuse state they do not fully understand
// (scenario "healing never revokes what it cannot read")
#[tokio::test]
async fn healing_never_revokes_a_future_version_grant() {
    // A grant declaring a newer schema version is kept by the lenient list
    // path and flagged needs_newer. Healing must leave it untouched: it reports
    // the grant as skipped-locked and issues no key resolution or revoke.
    let mut future_grant = serde_json::to_value(dummy_grant("did:plc:recipient")).unwrap();
    future_grant["opakeVersion"] = serde_json::json!(SCHEMA_VERSION + 1);

    let mock = MockTransport::new();
    mock.enqueue(list_grants_response(future_grant));
    let mut client = mock_client(mock);

    let result = heal_stale_grants(&mut client).await.unwrap();

    assert_eq!(result.grants_checked, 1);
    assert_eq!(result.grants_skipped_locked, 1, "future-version grant reported");
    assert_eq!(result.grants_deleted, 0, "never revoked");
    assert_eq!(result.grants_failed, 0);

    // Only the listRecords call happened — no recipient-key resolution, no
    // revoke. The write-strict guard short-circuits before any of that.
    let reqs = client.transport().requests();
    assert_eq!(reqs.len(), 1, "healing issued no write and no key lookup");
    assert!(
        reqs.iter().all(|r| !r.url.contains("deleteRecord")),
        "no grant deletion for a locked grant"
    );
}
