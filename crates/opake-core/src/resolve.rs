// Identity resolution: handle/DID → PDS → public key.
//
// Combines the unauthenticated client primitives into a single high-level
// call that takes a handle or DID string and returns everything needed to
// encrypt data for that user.

use log::debug;

use crate::client::{
    get_record_public, pds_from_did_document, resolve_did_document, resolve_handle, Transport,
    XrpcClient,
};
use crate::crypto::X25519PublicKey;
use crate::error::Error;
use crate::records::{self, PublicKeyRecord, PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY};

/// Ed25519 signing public key: 32 raw bytes.
pub type Ed25519PublicKeyBytes = [u8; 32];

/// Everything we learn about a remote user during resolution.
#[derive(Debug)]
pub struct ResolvedIdentity {
    pub did: String,
    pub handle: Option<String>,
    pub pds_url: String,
    pub public_key: X25519PublicKey,
    pub algo: String,
    /// Ed25519 signing key — present if the user has published one.
    pub signing_key: Option<Ed25519PublicKeyBytes>,
}

/// Full resolution: input → DID → PDS → public key.
///
/// If `input` starts with `did:`, it's used directly. Otherwise it's treated
/// as a handle and resolved against `caller_pds_url` first.
pub async fn resolve_identity(
    transport: &impl Transport,
    caller_pds_url: &str,
    input: &str,
) -> Result<ResolvedIdentity, Error> {
    // Step 1: Resolve to DID
    let did = if input.starts_with("did:") {
        debug!("input is already a DID: {}", input);
        input.to_string()
    } else {
        debug!("resolving handle: {}", input);
        resolve_handle(transport, caller_pds_url, input).await?
    };

    // Step 2: Fetch DID document
    debug!("fetching DID document for {}", did);
    let doc = resolve_did_document(transport, &did).await?;

    // Step 3: Extract PDS URL
    let pds_url = pds_from_did_document(&doc)?;
    debug!("PDS for {}: {}", did, pds_url);

    // Step 4: Extract handle from alsoKnownAs
    let handle = doc
        .also_known_as
        .iter()
        .find_map(|alias| alias.strip_prefix("at://"))
        .map(|h| h.to_string());

    // Step 5: Fetch public key record
    debug!("fetching public key from {}", pds_url);
    let entry = get_record_public(
        transport,
        &pds_url,
        &did,
        PUBLIC_KEY_COLLECTION,
        PUBLIC_KEY_RKEY,
    )
    .await?;

    let record: PublicKeyRecord = serde_json::from_value(entry.value)?;
    records::check_version(record.version)?;

    // Step 6: Decode and validate public key bytes
    let key_bytes = record
        .public_key
        .decode()
        .map_err(|e| Error::InvalidRecord(format!("invalid public key: {e}")))?;

    let public_key: [u8; 32] = key_bytes.try_into().map_err(|v: Vec<u8>| {
        Error::InvalidRecord(format!("public key is {} bytes, expected 32", v.len()))
    })?;

    // Step 7: Decode optional signing key
    let signing_key = match record.signing_key {
        Some(ref sk) => {
            let sk_bytes = sk
                .decode()
                .map_err(|e| Error::InvalidRecord(format!("invalid signing key: {e}")))?;
            let key: [u8; 32] = sk_bytes.try_into().map_err(|v: Vec<u8>| {
                Error::InvalidRecord(format!("signing key is {} bytes, expected 32", v.len()))
            })?;
            Some(key)
        }
        None => None,
    };

    Ok(ResolvedIdentity {
        did,
        handle,
        pds_url,
        public_key,
        algo: record.algo,
        signing_key,
    })
}

/// Publish (upsert) the user's encryption + signing public keys to their PDS.
///
/// Called on every login — `putRecord` is idempotent, so this is always
/// one request regardless of whether the record already exists.
pub async fn publish_public_key(
    client: &mut XrpcClient<impl Transport>,
    public_key: &X25519PublicKey,
    signing_key: Option<&Ed25519PublicKeyBytes>,
    created_at: &str,
) -> Result<String, Error> {
    let record = match signing_key {
        Some(sk) => PublicKeyRecord::with_signing_key(public_key, sk, created_at),
        None => PublicKeyRecord::new(public_key, created_at),
    };
    let result = client
        .put_record(PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY, &record)
        .await?;
    Ok(result.uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::HttpResponse;
    use crate::records::{PublicKeyRecord, SCHEMA_VERSION};
    use crate::test_utils::MockTransport;

    fn success(body: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: body.as_bytes().to_vec(),
        }
    }

    fn did_document_json(did: &str, handle: &str, pds_url: &str) -> String {
        serde_json::json!({
            "id": did,
            "alsoKnownAs": [format!("at://{handle}")],
            "service": [{
                "id": "#atproto_pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": pds_url,
            }]
        })
        .to_string()
    }

    fn public_key_record_json(public_key: &X25519PublicKey) -> String {
        let record = PublicKeyRecord::new(public_key, "2026-03-01T00:00:00Z");
        let entry = serde_json::json!({
            "uri": "at://did:plc:target/app.opake.cloud.publicKey/self",
            "cid": "bafyrecord",
            "value": record,
        });
        entry.to_string()
    }

    #[tokio::test]
    async fn resolve_from_handle() {
        let mock = MockTransport::new();
        let pubkey = [42u8; 32];

        // 1. resolveHandle → DID
        mock.enqueue(success(r#"{"did":"did:plc:target"}"#));
        // 2. DID document
        mock.enqueue(success(&did_document_json(
            "did:plc:target",
            "alice.test",
            "https://pds.alice.example.com",
        )));
        // 3. Public key record
        mock.enqueue(success(&public_key_record_json(&pubkey)));

        let result = resolve_identity(&mock, "https://pds.caller", "alice.test")
            .await
            .unwrap();

        assert_eq!(result.did, "did:plc:target");
        assert_eq!(result.handle.as_deref(), Some("alice.test"));
        assert_eq!(result.pds_url, "https://pds.alice.example.com");
        assert_eq!(result.public_key, pubkey);
        assert_eq!(result.algo, "x25519");

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 3);
        assert!(reqs[0].url.contains("resolveHandle"));
        assert!(reqs[1].url.contains("plc.directory"));
        assert!(reqs[2].url.contains("pds.alice.example.com"));
    }

    #[tokio::test]
    async fn resolve_from_did_skips_handle_resolution() {
        let mock = MockTransport::new();
        let pubkey = [7u8; 32];

        // Only 2 requests — no resolveHandle
        mock.enqueue(success(&did_document_json(
            "did:plc:bob",
            "bob.test",
            "https://pds.bob.example.com",
        )));
        mock.enqueue(success(&public_key_record_json(&pubkey)));

        let result = resolve_identity(&mock, "https://pds.caller", "did:plc:bob")
            .await
            .unwrap();

        assert_eq!(result.did, "did:plc:bob");
        assert_eq!(result.handle.as_deref(), Some("bob.test"));
        assert_eq!(result.public_key, pubkey);

        assert_eq!(mock.requests().len(), 2);
    }

    #[tokio::test]
    async fn no_public_key_record_returns_not_found() {
        let mock = MockTransport::new();
        mock.enqueue(success(&did_document_json(
            "did:plc:nopubkey",
            "ghost.test",
            "https://pds.ghost",
        )));
        mock.enqueue(HttpResponse {
            status: 404,
            body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
        });

        let err = resolve_identity(&mock, "https://pds.caller", "did:plc:nopubkey")
            .await
            .unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    #[tokio::test]
    async fn rejects_future_schema_version() {
        let mock = MockTransport::new();
        mock.enqueue(success(&did_document_json(
            "did:plc:future",
            "future.test",
            "https://pds.future",
        )));

        let mut record = PublicKeyRecord::new(&[1u8; 32], "2026-03-01T00:00:00Z");
        record.version = SCHEMA_VERSION + 1;
        let entry = serde_json::json!({
            "uri": "at://did:plc:future/app.opake.cloud.publicKey/self",
            "cid": "bafy",
            "value": record,
        });
        mock.enqueue(success(&entry.to_string()));

        let err = resolve_identity(&mock, "https://pds.caller", "did:plc:future")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("schema version"), "got: {err}");
    }

    #[tokio::test]
    async fn publish_public_key_puts_record_and_returns_uri() {
        let mock = MockTransport::new();
        let pubkey = [55u8; 32];

        let put_response = serde_json::json!({
            "uri": "at://did:plc:test/app.opake.cloud.publicKey/self",
            "cid": "bafypublished",
        });
        mock.enqueue(success(&put_response.to_string()));

        let session = crate::client::Session {
            did: "did:plc:test".into(),
            handle: "test.handle".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        };
        let mut client = XrpcClient::with_session(mock.clone(), "https://pds.test".into(), session);

        let signing_key = [88u8; 32];
        let uri = publish_public_key(
            &mut client,
            &pubkey,
            Some(&signing_key),
            "2026-03-01T12:00:00Z",
        )
        .await
        .unwrap();

        assert_eq!(uri, "at://did:plc:test/app.opake.cloud.publicKey/self");

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].url.contains("putRecord"));
    }

    #[tokio::test]
    async fn handle_without_at_prefix_gives_none() {
        let mock = MockTransport::new();
        let pubkey = [99u8; 32];

        // DID doc with no alsoKnownAs entries
        mock.enqueue(success(
            &serde_json::json!({
                "id": "did:plc:lonely",
                "service": [{
                    "id": "#atproto_pds",
                    "serviceEndpoint": "https://pds.lonely",
                }]
            })
            .to_string(),
        ));
        mock.enqueue(success(&public_key_record_json(&pubkey)));

        let result = resolve_identity(&mock, "https://pds.caller", "did:plc:lonely")
            .await
            .unwrap();
        assert!(result.handle.is_none());
    }
}
