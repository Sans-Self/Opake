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
        id: "did:plc:test".into(),
        also_known_as: vec!["https://example.com".into()],
        service: vec![],
    };
    assert_eq!(handle_from_did_document(&doc), None);
}

#[test]
fn handle_from_did_document_empty() {
    let doc = DidDocument {
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
