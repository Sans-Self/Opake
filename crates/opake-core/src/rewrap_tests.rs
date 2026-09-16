use super::*;

use base64::Engine;

use crate::atproto::{AtBytes, BlobRef, CidLink};
use crate::client::{HttpResponse, LegacySession, Session, XrpcClient};
use crate::crypto::ContentKey;
use crate::records::{
    Document, EncryptedMetadata, Encryption, KeyringEncryption, KeyringRef, SCHEMA_VERSION,
};
use crate::test_utils::MockTransport;
use crate::workspace::{GroupKeys, HistoricalKey};

const TEST_DID: &str = "did:plc:test";
const WS_URI: &str = "at://did:plc:test/at.opake.keyring/ws";
const DOC_URI: &str = "at://did:plc:test/at.opake.document/doc1";

fn key(seed: u8) -> ContentKey {
    ContentKey([seed; 32])
}

fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
    let session = Session::Legacy(LegacySession {
        did: TEST_DID.into(),
        handle: "test.handle".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    XrpcClient::with_session(mock, "https://pds.test".into(), session)
}

/// A keyring-encrypted document whose content key `ck` is wrapped under
/// `group_key` and tagged at `rotation`.
fn keyring_document(
    workspace_id: &str,
    group_key: &ContentKey,
    rotation: u64,
    ck: &ContentKey,
) -> Document {
    let wrapped = crate::crypto::wrap_content_key_for_keyring(ck, group_key).unwrap();
    Document {
        opake_version: SCHEMA_VERSION,
        blob: BlobRef {
            blob_type: "blob".into(),
            reference: CidLink {
                cid: "bafyblob".into(),
            },
            mime_type: "application/octet-stream".into(),
            size: 0,
        },
        encryption: Encryption::Keyring(KeyringEncryption {
            keyring_ref: KeyringRef {
                keyring: workspace_id.into(),
                wrapped_content_key: AtBytes {
                    encoded: base64::engine::general_purpose::STANDARD.encode(&wrapped),
                },
                rotation,
            },
            algo: "aes-256-gcm".into(),
            nonce: AtBytes {
                encoded: String::new(),
            },
        }),
        encrypted_metadata: EncryptedMetadata {
            ciphertext: AtBytes {
                encoded: String::new(),
            },
            nonce: AtBytes {
                encoded: String::new(),
            },
        },
        supersedes: None,
        supersedes_cid: None,
        lineage: None,
        workspace_id: Some(workspace_id.into()),
        created_at: "2026-04-17T00:00:00Z".into(),
        modified_at: None,
    }
}

fn get_record_response(document: &Document, cid: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&serde_json::json!({
            "uri": DOC_URI,
            "cid": cid,
            "value": document,
        }))
        .unwrap(),
    }
}

/// Extract the keyring rotation + wrapped content key from a document.
fn keyring_ref(document: &Document) -> &KeyringRef {
    match &document.encryption {
        Encryption::Keyring(ke) => &ke.keyring_ref,
        _ => panic!("expected keyring encryption"),
    }
}

// A trailing wrap re-wraps to the head and the migrated key still unwraps to
// the original content key; a wrap already at the head is left alone.
// spec:key-rotation § The re-wrap sweep is hygiene under the background-work contract
#[test]
fn plan_rewrap_migrates_trailing_wrap_to_head() {
    let (k0, k1) = (key(0), key(1));
    let ck = key(42);
    let historical = vec![HistoricalKey {
        rotation: 0,
        key: k0.clone(),
    }];
    let head = GroupKeys {
        current_rotation: 1,
        current: Some(&k1),
        historical: &historical,
    };

    let doc = keyring_document(WS_URI, &k0, 0, &ck);
    let plan = plan_rewrap(&doc, WS_URI, head).unwrap();
    let RewrapPlan::Rewrap(rewrapped) = plan else {
        panic!("expected a re-wrap, got {plan:?}");
    };

    let kr = keyring_ref(&rewrapped);
    assert_eq!(kr.rotation, 1, "wrap advanced to the head rotation");

    // The migrated wrap round-trips to the same content key under the head.
    let bytes = kr.wrapped_content_key.decode().unwrap();
    let recovered = crate::crypto::unwrap_content_key_from_keyring(&bytes, &k1).unwrap();
    assert_eq!(recovered.0, ck.0, "content key preserved across re-wrap");
}

#[test]
fn plan_rewrap_leaves_head_and_foreign_docs_alone() {
    let (k0, k1) = (key(0), key(1));
    let ck = key(42);
    let historical = vec![HistoricalKey {
        rotation: 0,
        key: k0.clone(),
    }];
    let head = GroupKeys {
        current_rotation: 1,
        current: Some(&k1),
        historical: &historical,
    };

    // Already at the head rotation.
    let current = keyring_document(WS_URI, &k1, 1, &ck);
    assert!(matches!(
        plan_rewrap(&current, WS_URI, head).unwrap(),
        RewrapPlan::AlreadyCurrent
    ));

    // A document for a different workspace is not this sweep's concern.
    let foreign = keyring_document("at://did:plc:other/at.opake.keyring/x", &k0, 0, &ck);
    assert!(matches!(
        plan_rewrap(&foreign, WS_URI, head).unwrap(),
        RewrapPlan::NotApplicable
    ));
}

// The plan targets whatever head it is given — a head that advanced mid-sweep
// yields a re-wrap to the newer rotation, never to a superseded one.
// spec:background-work § Concurrency is resolved per record by compare-and-swap
#[test]
fn plan_rewrap_targets_the_head_resolved_at_write_time() {
    let (k1, k2, k3) = (key(1), key(2), key(3));
    let ck = key(42);
    let doc = keyring_document(WS_URI, &k1, 1, &ck);

    // Head at rotation 2 when the item is planned.
    let hist_2 = vec![HistoricalKey {
        rotation: 1,
        key: k1.clone(),
    }];
    let head_2 = GroupKeys {
        current_rotation: 2,
        current: Some(&k2),
        historical: &hist_2,
    };
    let RewrapPlan::Rewrap(at_2) = plan_rewrap(&doc, WS_URI, head_2).unwrap() else {
        panic!("expected re-wrap at rotation 2");
    };
    assert_eq!(keyring_ref(&at_2).rotation, 2);

    // The keyring advanced to 3 before the item was reached: the same trailing
    // document now re-wraps to 3, not to the now-superseded 2.
    let hist_3 = vec![
        HistoricalKey {
            rotation: 1,
            key: k1.clone(),
        },
        HistoricalKey {
            rotation: 2,
            key: k2.clone(),
        },
    ];
    let head_3 = GroupKeys {
        current_rotation: 3,
        current: Some(&k3),
        historical: &hist_3,
    };
    let RewrapPlan::Rewrap(at_3) = plan_rewrap(&doc, WS_URI, head_3).unwrap() else {
        panic!("expected re-wrap at rotation 3");
    };
    assert_eq!(
        keyring_ref(&at_3).rotation,
        3,
        "a head that moved mid-sweep re-targets the current rotation"
    );
}

// A mixed set derives exactly the trailing remainder: docs behind the head are
// re-wrapped, docs at the head are skipped as already current.
// spec:key-rotation § The re-wrap sweep is hygiene under the background-work contract
#[test]
fn sweep_derives_only_the_trailing_remainder() {
    let (k0, k1) = (key(0), key(1));
    let historical = vec![HistoricalKey {
        rotation: 0,
        key: k0.clone(),
    }];
    let head = GroupKeys {
        current_rotation: 1,
        current: Some(&k1),
        historical: &historical,
    };

    let docs = [
        keyring_document(WS_URI, &k0, 0, &key(10)), // trailing → rewrap
        keyring_document(WS_URI, &k1, 1, &key(11)), // current → skip
        keyring_document(WS_URI, &k0, 0, &key(12)), // trailing → rewrap
    ];
    let (mut rewrap, mut current) = (0, 0);
    for doc in &docs {
        match plan_rewrap(doc, WS_URI, head).unwrap() {
            RewrapPlan::Rewrap(_) => rewrap += 1,
            RewrapPlan::AlreadyCurrent => current += 1,
            RewrapPlan::NotApplicable => {}
        }
    }
    assert_eq!(rewrap, 2, "both trailing wraps derived");
    assert_eq!(current, 1, "the head-rotation wrap is not re-derived");
}

// A CAS conflict on the write is a skip, not an error — another runner won the
// item, and re-deriving finds nothing to do.
// spec:background-work § Concurrency is resolved per record by compare-and-swap
#[tokio::test]
async fn rewrap_document_skips_on_cas_conflict() {
    let (k0, k1) = (key(0), key(1));
    let ck = key(42);
    let doc = keyring_document(WS_URI, &k0, 0, &ck);

    let mock = MockTransport::new();
    mock.enqueue(get_record_response(&doc, "bafyDOC"));
    mock.enqueue(HttpResponse {
        status: 400,
        headers: vec![],
        body: br#"{"error":"InvalidSwap","message":"Record was at a different CID"}"#.to_vec(),
    });
    let mut client = mock_client(mock);

    let historical = vec![HistoricalKey {
        rotation: 0,
        key: k0.clone(),
    }];
    let head = GroupKeys {
        current_rotation: 1,
        current: Some(&k1),
        historical: &historical,
    };

    let item = rewrap_document_to_head(&mut client, WS_URI, DOC_URI, head)
        .await
        .expect("CAS conflict must not surface as an error");
    assert_eq!(item, RewrapItem::Conflict);
}

// The write is conditioned on the CID just read, and lands the migrated record.
// spec:background-work § Concurrency is resolved per record by compare-and-swap
#[tokio::test]
async fn rewrap_document_conditions_write_on_read_cid() {
    use crate::client::RequestBody;

    let (k0, k1) = (key(0), key(1));
    let ck = key(42);
    let doc = keyring_document(WS_URI, &k0, 0, &ck);

    let mock = MockTransport::new();
    mock.enqueue(get_record_response(&doc, "bafyDOC"));
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: serde_json::to_vec(&serde_json::json!({ "uri": DOC_URI, "cid": "bafyNEW" })).unwrap(),
    });
    let mut client = mock_client(mock.clone());

    let historical = vec![HistoricalKey {
        rotation: 0,
        key: k0.clone(),
    }];
    let head = GroupKeys {
        current_rotation: 1,
        current: Some(&k1),
        historical: &historical,
    };

    let item = rewrap_document_to_head(&mut client, WS_URI, DOC_URI, head)
        .await
        .unwrap();
    assert_eq!(item, RewrapItem::Rewrapped);

    let reqs = mock.requests();
    let put = reqs.last().unwrap();
    assert!(put.url.contains("putRecord"));
    let body = match &put.body {
        Some(RequestBody::Json(v)) => v.clone(),
        _ => panic!("expected JSON body"),
    };
    assert_eq!(body["swapRecord"], "bafyDOC", "CAS conditioned on read CID");
    assert_eq!(
        body["record"]["encryption"]["keyringRef"]["rotation"], 1,
        "written record advanced to the head rotation"
    );
}
