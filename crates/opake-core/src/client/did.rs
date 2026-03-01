// DID resolution and unauthenticated cross-PDS queries.
//
// These free functions take a bare `&impl Transport` — no XrpcClient, no auth.
// Used for resolving handles, fetching DID documents, and reading public
// records from other users' PDSes.

use log::debug;
use serde::Deserialize;

use super::transport::*;
use super::xrpc::{check_response, RecordEntry};
use crate::error::Error;

// ---------------------------------------------------------------------------
// DID document types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DidDocument {
    pub id: String,
    #[serde(default, rename = "alsoKnownAs")]
    pub also_known_as: Vec<String>,
    #[serde(default)]
    pub service: Vec<DidService>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DidService {
    pub id: String,
    #[serde(rename = "serviceEndpoint")]
    pub service_endpoint: String,
}

// ---------------------------------------------------------------------------
// Unauthenticated free functions
// ---------------------------------------------------------------------------

/// Resolve a handle to a DID via `com.atproto.identity.resolveHandle`.
/// Unauthenticated — can be called against any PDS.
pub async fn resolve_handle(
    transport: &impl Transport,
    pds_url: &str,
    handle: &str,
) -> Result<String, Error> {
    debug!("resolving handle {} via {}", handle, pds_url);

    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url: format!(
                "{}/xrpc/com.atproto.identity.resolveHandle?handle={}",
                pds_url, handle,
            ),
            headers: vec![],
            body: None,
        })
        .await?;

    check_response(&response)?;

    #[derive(Deserialize)]
    struct ResolveResponse {
        did: String,
    }

    let parsed: ResolveResponse = serde_json::from_slice(&response.body)?;
    Ok(parsed.did)
}

/// Fetch a single record from any PDS. Unauthenticated.
pub async fn get_record_public(
    transport: &impl Transport,
    pds_url: &str,
    did: &str,
    collection: &str,
    rkey: &str,
) -> Result<RecordEntry, Error> {
    debug!(
        "fetching public record {}/{}/{} from {}",
        did, collection, rkey, pds_url,
    );

    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url: format!(
                "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey={}",
                pds_url, did, collection, rkey,
            ),
            headers: vec![],
            body: None,
        })
        .await?;

    check_response(&response)?;
    Ok(serde_json::from_slice(&response.body)?)
}

/// Fetch a blob from any PDS by DID + CID. Unauthenticated.
pub async fn get_blob_public(
    transport: &impl Transport,
    pds_url: &str,
    did: &str,
    cid: &str,
) -> Result<Vec<u8>, Error> {
    debug!(
        "fetching public blob did={} cid={} from {}",
        did, cid, pds_url
    );

    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url: format!(
                "{}/xrpc/com.atproto.sync.getBlob?did={}&cid={}",
                pds_url, did, cid,
            ),
            headers: vec![],
            body: None,
        })
        .await?;

    check_response(&response)?;
    Ok(response.body)
}

const PLC_DIRECTORY: &str = "https://plc.directory";

/// Fetch a DID document from the PLC directory (did:plc) or .well-known (did:web).
pub async fn resolve_did_document(
    transport: &impl Transport,
    did: &str,
) -> Result<DidDocument, Error> {
    let url = if did.starts_with("did:plc:") {
        format!("{PLC_DIRECTORY}/{did}")
    } else if let Some(domain) = did.strip_prefix("did:web:") {
        format!("https://{domain}/.well-known/did.json")
    } else {
        return Err(Error::InvalidRecord(format!(
            "unsupported DID method: {did}"
        )));
    };

    debug!("fetching DID document from {}", url);

    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url,
            headers: vec![],
            body: None,
        })
        .await?;

    check_response(&response)?;
    Ok(serde_json::from_slice(&response.body)?)
}

/// Extract the PDS service endpoint (`#atproto_pds`) from a DID document.
pub fn pds_from_did_document(doc: &DidDocument) -> Result<String, Error> {
    doc.service
        .iter()
        .find(|s| s.id == "#atproto_pds")
        .map(|s| s.service_endpoint.clone())
        .ok_or_else(|| {
            Error::NotFound(format!(
                "no #atproto_pds service in DID document for {}",
                doc.id,
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::MockTransport;

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            body: body.as_bytes().to_vec(),
        }
    }

    fn success_response(body: &str) -> HttpResponse {
        response(200, body)
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
}
