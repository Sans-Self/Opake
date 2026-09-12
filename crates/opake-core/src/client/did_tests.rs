use super::*;
use crate::test_utils::MockTransport;

fn response(status: u16, body: &str) -> HttpResponse {
    HttpResponse {
        status,
        headers: vec![],
        body: body.as_bytes().to_vec(),
    }
}

fn success_response(body: &str) -> HttpResponse {
    response(200, body)
}

fn base58btc(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut digits = vec![0u8];
    for &byte in bytes {
        let mut carry = u32::from(byte);
        for digit in digits.iter_mut().rev() {
            carry += u32::from(*digit) << 8;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry != 0 {
            digits.insert(0, (carry % 58) as u8);
            carry /= 58;
        }
    }
    let leading_zeros = bytes.iter().take_while(|&&byte| byte == 0).count();
    let mut output = String::with_capacity(leading_zeros + digits.len());
    output.extend(std::iter::repeat_n('1', leading_zeros));
    output.extend(
        digits
            .into_iter()
            .map(|digit| ALPHABET[digit as usize] as char),
    );
    output
}

fn did_key(key: &[u8; 32]) -> String {
    let mut multicodec = vec![0xed, 0x01];
    multicodec.extend_from_slice(key);
    format!("did:key:z{}", base58btc(&multicodec))
}

#[test]
fn opake_method_lookup_distinguishes_absent_malformed_and_unsupported() {
    let signing = ed25519_dalek::SigningKey::from_bytes(&[7; 32]);
    let key = signing.verifying_key().to_bytes();
    let did = "did:plc:test";

    let absent: DidDocument = serde_json::from_value(serde_json::json!({"id": did})).unwrap();
    assert_eq!(absent.opake_key().unwrap(), None);

    let malformed: DidDocument = serde_json::from_value(serde_json::json!({
        "id": did,
        "verificationMethod": [{"id": "#opake", "type": "Multikey"}],
    }))
    .unwrap();
    assert!(matches!(
        malformed.opake_key(),
        Err(VerificationMethodError::Malformed(_))
    ));

    let unsupported: DidDocument = serde_json::from_value(serde_json::json!({
        "id": did,
        "verificationMethod": [{
            "id": "#opake", "controller": did, "type": "P256Key",
            "publicKeyMultibase": did_key(&key).strip_prefix("did:key:").unwrap(),
        }],
    }))
    .unwrap();
    assert_eq!(
        unsupported.opake_key(),
        Err(VerificationMethodError::UnsupportedKeyType)
    );
}

#[test]
fn opake_method_decodes_ed25519_multibase_and_rejects_wrong_codec() {
    let signing = ed25519_dalek::SigningKey::from_bytes(&[8; 32]);
    let key = signing.verifying_key().to_bytes();
    assert_eq!(
        decode_ed25519_multibase(did_key(&key).strip_prefix("did:key:").unwrap()).unwrap(),
        key
    );

    let mut wrong_codec = vec![0xec, 0x01];
    wrong_codec.extend_from_slice(&key);
    assert_eq!(
        decode_ed25519_multibase(&format!("z{}", base58btc(&wrong_codec))),
        Err(VerificationMethodError::UnsupportedKeyType),
    );
}

#[test]
fn ed25519_did_key_encoding_round_trips() {
    let key = ed25519_dalek::SigningKey::from_bytes(&[42; 32])
        .verifying_key()
        .to_bytes();
    let encoded = encode_ed25519_did_key(&key);
    assert_eq!(encoded, did_key(&key));
    assert_eq!(
        decode_ed25519_multibase(encoded.strip_prefix("did:key:").unwrap()).unwrap(),
        key
    );
}

#[tokio::test]
async fn plc_history_uses_active_operations_and_detects_replacement() {
    let did = "did:plc:test";
    let first = ed25519_dalek::SigningKey::from_bytes(&[9; 32])
        .verifying_key()
        .to_bytes();
    let current = ed25519_dalek::SigningKey::from_bytes(&[10; 32])
        .verifying_key()
        .to_bytes();
    let mock = MockTransport::new();
    mock.enqueue(success_response(
        &serde_json::json!([
            {"type": "plc_operation", "verificationMethods": {"opake": did_key(&first)}},
            {"type": "plc_operation", "verificationMethods": {}},
            {"type": "plc_operation", "verificationMethods": {"opake": did_key(&current)}},
        ])
        .to_string(),
    ));

    assert_eq!(
        opake_key_replaced(&mock, did, &current).await.unwrap(),
        Some(true)
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].url.ends_with("/did:plc:test/log"));
}

#[tokio::test]
async fn plc_history_removal_and_same_key_readdition_is_not_replacement() {
    let did = "did:plc:test";
    let current = ed25519_dalek::SigningKey::from_bytes(&[11; 32])
        .verifying_key()
        .to_bytes();
    let mock = MockTransport::new();
    mock.enqueue(success_response(
        &serde_json::json!([
            {"type": "plc_operation", "verificationMethods": {"opake": did_key(&current)}},
            {"type": "plc_operation", "verificationMethods": {}},
            {"type": "plc_operation", "verificationMethods": {"opake": did_key(&current)}},
        ])
        .to_string(),
    ));

    assert_eq!(
        opake_key_replaced(&mock, did, &current).await.unwrap(),
        Some(false)
    );
}

#[tokio::test]
async fn did_web_has_no_plc_history_and_malformed_history_refuses() {
    let key = ed25519_dalek::SigningKey::from_bytes(&[12; 32])
        .verifying_key()
        .to_bytes();
    let mock = MockTransport::new();
    assert_eq!(
        opake_key_replaced(&mock, "did:web:example.test", &key)
            .await
            .unwrap(),
        None
    );
    assert!(mock.requests().is_empty());

    mock.enqueue(success_response(r#"[{"verificationMethods": []}]"#));
    assert!(opake_key_replaced(&mock, "did:plc:test", &key)
        .await
        .is_err());
}

// -- resolve_handle_wellknown --

#[tokio::test]
async fn resolve_wellknown_happy_path() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: b"did:plc:abc123".to_vec(),
    });

    let did = resolve_handle_wellknown(&mock, "alice.test").await.unwrap();
    assert_eq!(did, "did:plc:abc123");

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].url, "https://alice.test/.well-known/atproto-did");
}

#[tokio::test]
async fn resolve_wellknown_trims_whitespace() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: b"  did:plc:abc123\n".to_vec(),
    });

    let did = resolve_handle_wellknown(&mock, "alice.test").await.unwrap();
    assert_eq!(did, "did:plc:abc123");
}

#[tokio::test]
async fn resolve_wellknown_rejects_non_did() {
    let mock = MockTransport::new();
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: b"not-a-did".to_vec(),
    });

    let err = resolve_handle_wellknown(&mock, "alice.test")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not a DID"));
}

#[tokio::test]
async fn resolve_wellknown_404() {
    let mock = MockTransport::new();
    mock.enqueue(response(404, "Not Found"));

    let err = resolve_handle_wellknown(&mock, "noserver.test")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

// -- resolve_handle --

#[tokio::test]
async fn resolve_handle_happy_path() {
    let mock = MockTransport::new();
    mock.enqueue(success_response(r#"{"did":"did:plc:abc123"}"#));

    let did = resolve_handle(&mock, "https://pds.test", "alice.test")
        .await
        .unwrap();
    assert_eq!(did, "did:plc:abc123");

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].url.contains("resolveHandle"));
    assert!(reqs[0].url.contains("handle=alice.test"));
    assert!(reqs[0].headers.is_empty());
}

#[tokio::test]
async fn resolve_handle_not_found() {
    let mock = MockTransport::new();
    mock.enqueue(response(
        400,
        r#"{"error":"InvalidHandle","message":"Unable to resolve handle"}"#,
    ));

    let err = resolve_handle(&mock, "https://pds.test", "nobody.fake")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Xrpc { status: 400, .. }));
}

// -- get_record_public --

#[tokio::test]
async fn get_record_public_happy_path() {
    let mock = MockTransport::new();
    mock.enqueue(success_response(
        r#"{"uri":"at://did:plc:abc/col/rkey","cid":"bafy","value":{"hello":"world"}}"#,
    ));

    let entry = get_record_public(&mock, "https://pds.other", "did:plc:abc", "col", "rkey")
        .await
        .unwrap();
    assert_eq!(entry.uri, "at://did:plc:abc/col/rkey");
    assert_eq!(entry.value["hello"], "world");

    let reqs = mock.requests();
    assert!(reqs[0].url.starts_with("https://pds.other"));
    assert!(reqs[0].headers.is_empty());
}

#[tokio::test]
async fn get_record_public_404() {
    let mock = MockTransport::new();
    mock.enqueue(response(
        404,
        r#"{"error":"RecordNotFound","message":"not found"}"#,
    ));

    let err = get_record_public(&mock, "https://pds.other", "did:plc:abc", "col", "rkey")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

// -- get_blob_public --

#[tokio::test]
async fn get_blob_public_happy_path() {
    let mock = MockTransport::new();
    let blob_data = b"encrypted-blob-bytes";
    mock.enqueue(HttpResponse {
        status: 200,
        headers: vec![],
        body: blob_data.to_vec(),
    });

    let data = get_blob_public(&mock, "https://pds.owner", "did:plc:owner", "bafyblob123")
        .await
        .unwrap();
    assert_eq!(data, blob_data);

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].url.starts_with("https://pds.owner"));
    assert!(reqs[0].url.contains("getBlob"));
    assert!(reqs[0].url.contains("did=did:plc:owner"));
    assert!(reqs[0].url.contains("cid=bafyblob123"));
    assert!(reqs[0].headers.is_empty());
}

#[tokio::test]
async fn get_blob_public_404() {
    let mock = MockTransport::new();
    mock.enqueue(response(
        404,
        r#"{"error":"BlobNotFound","message":"not found"}"#,
    ));

    let err = get_blob_public(&mock, "https://pds.owner", "did:plc:abc", "bafymissing")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

// -- resolve_did_document --

fn plc_document_json() -> String {
    serde_json::json!({
        "id": "did:plc:abc123",
        "alsoKnownAs": ["at://alice.test"],
        "service": [{
            "id": "#atproto_pds",
            "type": "AtprotoPersonalDataServer",
            "serviceEndpoint": "https://pds.alice.example.com"
        }]
    })
    .to_string()
}

#[tokio::test]
async fn resolve_did_document_plc() {
    let mock = MockTransport::new();
    mock.enqueue(success_response(&plc_document_json()));

    let doc = resolve_did_document(&mock, "did:plc:abc123").await.unwrap();
    assert_eq!(doc.id, "did:plc:abc123");
    assert_eq!(doc.also_known_as, vec!["at://alice.test"]);
    assert_eq!(doc.service.len(), 1);
    assert_eq!(doc.service[0].id, "#atproto_pds");

    let reqs = mock.requests();
    assert!(reqs[0].url.contains("plc.directory/did:plc:abc123"));
}

#[tokio::test]
async fn resolve_did_document_web() {
    let mock = MockTransport::new();
    mock.enqueue(success_response(
        &serde_json::json!({
            "id": "did:web:example.com",
            "service": [{
                "id": "#atproto_pds",
                "serviceEndpoint": "https://pds.example.com"
            }]
        })
        .to_string(),
    ));

    let doc = resolve_did_document(&mock, "did:web:example.com")
        .await
        .unwrap();
    assert_eq!(doc.id, "did:web:example.com");

    let reqs = mock.requests();
    assert!(reqs[0].url.contains("example.com/.well-known/did.json"));
}

#[tokio::test]
async fn resolve_did_document_unsupported_method() {
    let mock = MockTransport::new();
    let err = resolve_did_document(&mock, "did:key:z123")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unsupported DID method"));
}

// -- pds_from_did_document --

#[test]
fn pds_from_did_document_extracts_endpoint() {
    let doc: DidDocument = serde_json::from_str(&plc_document_json()).unwrap();
    let pds = pds_from_did_document(&doc).unwrap();
    assert_eq!(pds, "https://pds.alice.example.com");
}

#[test]
fn pds_from_did_document_no_service() {
    let doc = DidDocument {
        verification_methods: vec![],
        id: "did:plc:test".into(),
        also_known_as: vec![],
        service: vec![],
    };
    let err = pds_from_did_document(&doc).unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
    assert!(err.to_string().contains("#atproto_pds"));
}

#[test]
fn pds_from_did_document_wrong_service_id() {
    let doc = DidDocument {
        verification_methods: vec![],
        id: "did:plc:test".into(),
        also_known_as: vec![],
        service: vec![DidService {
            id: "#something_else".into(),
            service_endpoint: "https://other.example.com".into(),
        }],
    };
    assert!(pds_from_did_document(&doc).is_err());
}

// -- did_document_url --

#[test]
fn did_document_url_plc() {
    let url = did_document_url("did:plc:abc123").unwrap();
    assert_eq!(url, "https://plc.directory/did:plc:abc123");
}

#[test]
fn did_document_url_web() {
    let url = did_document_url("did:web:example.com").unwrap();
    assert_eq!(url, "https://example.com/.well-known/did.json");
}

#[test]
fn did_document_url_unsupported() {
    let err = did_document_url("did:key:z123").unwrap_err();
    assert!(err.to_string().contains("unsupported DID method"));
}

// -- handle_from_did_document --

#[test]
fn handle_from_did_document_extracts_handle() {
    let doc: DidDocument = serde_json::from_str(&plc_document_json()).unwrap();
    assert_eq!(handle_from_did_document(&doc), Some("alice.test".into()));
}

#[test]
fn handle_from_did_document_no_at_entry() {
    let doc = DidDocument {
        verification_methods: vec![],
        id: "did:plc:test".into(),
        also_known_as: vec!["https://example.com".into()],
        service: vec![],
    };
    assert_eq!(handle_from_did_document(&doc), None);
}

#[test]
fn handle_from_did_document_empty() {
    let doc = DidDocument {
        verification_methods: vec![],
        id: "did:plc:test".into(),
        also_known_as: vec![],
        service: vec![],
    };
    assert_eq!(handle_from_did_document(&doc), None);
}

#[test]
fn resolve_plc_base_default() {
    assert_eq!(resolve_plc_base(None, None), "https://plc.directory");
}

#[test]
fn resolve_plc_base_env_beats_default() {
    assert_eq!(
        resolve_plc_base(None, Some("http://localhost:2582")),
        "http://localhost:2582"
    );
}

#[test]
fn resolve_plc_base_override_beats_env() {
    assert_eq!(
        resolve_plc_base(
            Some("http://plc.dev.internal"),
            Some("http://localhost:2582")
        ),
        "http://plc.dev.internal"
    );
}

#[test]
fn resolve_plc_base_strips_trailing_slash() {
    assert_eq!(
        resolve_plc_base(Some("http://localhost:2582/"), None),
        "http://localhost:2582"
    );
}
