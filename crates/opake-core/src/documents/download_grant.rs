use log::debug;

use crate::atproto;
use crate::client::{
    get_blob_public, get_record_public, pds_from_did_document, resolve_did_document, Transport,
};
use crate::crypto::{self, X25519PrivateKey};
use crate::error::Error;
use crate::records::{self, Document, Grant};
use crate::sharing::GRANT_COLLECTION;

use super::download::{decrypt_with_envelope, resolve_document_name};

/// Download and decrypt a file using a grant URI.
///
/// This is the cross-PDS path: the grant and document live on the *owner's*
/// PDS, not the caller's. All fetches are unauthenticated (public endpoints).
///
/// Temporary: recipients must pass the grant URI explicitly. This will be
/// replaced by automatic grant discovery once the `inbox` command exists.
pub async fn download_from_grant(
    transport: &impl Transport,
    private_key: &X25519PrivateKey,
    grant_uri: &str,
) -> Result<(String, Vec<u8>), Error> {
    let grant_at = atproto::parse_at_uri(grant_uri)?;
    if grant_at.collection != GRANT_COLLECTION {
        return Err(Error::InvalidRecord(format!(
            "expected a grant URI ({}), got collection {}",
            GRANT_COLLECTION, grant_at.collection,
        )));
    }

    // Resolve the owner's PDS from their DID
    let owner_did = &grant_at.authority;
    debug!("resolving PDS for owner {}", owner_did);
    let did_doc = resolve_did_document(transport, owner_did).await?;
    let owner_pds = pds_from_did_document(&did_doc)?;

    // Fetch the grant record
    debug!("fetching grant from {}", owner_pds);
    let grant_entry = get_record_public(
        transport,
        &owner_pds,
        owner_did,
        GRANT_COLLECTION,
        &grant_at.rkey,
    )
    .await?;

    let grant: Grant = serde_json::from_value(grant_entry.value)?;
    records::check_version(grant.opake_version)?;

    // Unwrap the content key from the grant
    debug!("unwrapping content key from grant");
    let content_key = crypto::unwrap_key(&grant.wrapped_key, private_key)?;

    // Fetch the document record
    let doc_at = atproto::parse_at_uri(&grant.document)?;
    debug!("fetching document from {}", owner_pds);
    let doc_entry = get_record_public(
        transport,
        &owner_pds,
        &doc_at.authority,
        &doc_at.collection,
        &doc_at.rkey,
    )
    .await?;

    let doc: Document = serde_json::from_value(doc_entry.value)?;
    records::check_version(doc.opake_version)?;

    // Grants always wrap the content key directly — the document's own
    // encryption type doesn't matter for the grant path, we just need the nonce.
    let envelope = match &doc.encryption {
        records::Encryption::Direct(d) => &d.envelope,
        records::Encryption::Keyring(_) => {
            return Err(Error::InvalidRecord(
                "grant-based download of keyring-encrypted documents is not supported — \
                 use keyring membership instead"
                    .into(),
            ));
        }
    };

    // Fetch the blob
    debug!(
        "fetching blob did={} cid={}",
        doc_at.authority, doc.blob.reference.cid
    );
    let ciphertext = get_blob_public(
        transport,
        &owner_pds,
        &doc_at.authority,
        &doc.blob.reference.cid,
    )
    .await?;

    let plaintext = decrypt_with_envelope(&content_key, envelope, ciphertext)?;
    let name = resolve_document_name(&doc, &content_key)?;
    Ok((name, plaintext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::HttpResponse;
    use crate::crypto::{OsRng, X25519PublicKey};
    use crate::records::{
        AtBytes, BlobRef, CidLink, DirectEncryption, EncryptedMetadata, Encryption,
        EncryptionEnvelope,
    };
    use crate::test_utils::MockTransport;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    const OWNER_DID: &str = "did:plc:owner";
    const OWNER_PDS: &str = "https://pds.owner.example.com";
    const GRANT_URI: &str = "at://did:plc:owner/app.opake.grant/grant1";
    const DOC_URI: &str = "at://did:plc:owner/app.opake.document/doc1";

    fn did_document_response() -> HttpResponse {
        let body = serde_json::json!({
            "id": OWNER_DID,
            "alsoKnownAs": ["at://owner.test"],
            "service": [{
                "id": "#atproto_pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": OWNER_PDS,
            }]
        });
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn grant_record_response(grant: &Grant) -> HttpResponse {
        let body = serde_json::json!({
            "uri": GRANT_URI,
            "cid": "bafygrant",
            "value": grant,
        });
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn record_response(doc: &Document) -> HttpResponse {
        let body = serde_json::to_vec(&serde_json::json!({
            "uri": DOC_URI,
            "cid": "bafyrecord",
            "value": doc,
        }))
        .unwrap();
        HttpResponse {
            status: 200,
            headers: vec![],
            body,
        }
    }

    fn blob_response(data: &[u8]) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: data.to_vec(),
        }
    }

    struct GrantFixture {
        payload: crypto::EncryptedPayload,
        owner_wrapped: records::WrappedKey,
        recipient_wrapped: records::WrappedKey,
        content_key: crypto::ContentKey,
    }

    fn encrypt_and_wrap(plaintext: &[u8], recipient_public: &X25519PublicKey) -> GrantFixture {
        let content_key = crypto::generate_content_key(&mut OsRng);
        let payload = crypto::encrypt_blob(&content_key, plaintext, &mut OsRng).unwrap();
        let owner_wrapped =
            crypto::wrap_key(&content_key, &[99u8; 32], OWNER_DID, &mut OsRng).unwrap();
        let recipient_wrapped = crypto::wrap_key(
            &content_key,
            recipient_public,
            "did:plc:recipient",
            &mut OsRng,
        )
        .unwrap();
        GrantFixture {
            payload,
            owner_wrapped,
            recipient_wrapped,
            content_key,
        }
    }

    fn make_document(
        name: &str,
        ciphertext_len: usize,
        nonce: &[u8; 12],
        owner_wrapped: records::WrappedKey,
        content_key: &crypto::ContentKey,
    ) -> Document {
        let metadata = crypto::DocumentMetadata {
            name: name.into(),
            mime_type: Some("text/plain".into()),
            size: Some(42),
            tags: vec![],
            description: None,
        };
        let encrypted_metadata =
            crypto::encrypt_metadata(content_key, &metadata, &mut OsRng).unwrap();

        Document {
            mime_type: Some("text/plain".into()),
            size: Some(42),
            ..Document::new(
                "encrypted".into(),
                BlobRef {
                    blob_type: "blob".into(),
                    reference: CidLink {
                        cid: "bafyblob".into(),
                    },
                    mime_type: "application/octet-stream".into(),
                    size: ciphertext_len as u64,
                },
                Encryption::Direct(DirectEncryption {
                    envelope: EncryptionEnvelope {
                        algo: "aes-256-gcm".into(),
                        nonce: AtBytes {
                            encoded: BASE64.encode(nonce),
                        },
                        keys: vec![owner_wrapped],
                    },
                }),
                encrypted_metadata,
                "2026-03-01T00:00:00Z".into(),
            )
        }
    }

    #[tokio::test]
    async fn roundtrip() {
        let recipient_secret = crypto::X25519DalekStaticSecret::random_from_rng(OsRng);
        let recipient_public = crypto::X25519DalekPublicKey::from(&recipient_secret);
        let recipient_private = recipient_secret.to_bytes();

        let plaintext = b"shared secret content";
        let fixture = encrypt_and_wrap(plaintext, recipient_public.as_bytes());

        let doc = make_document(
            "shared-file.txt",
            fixture.payload.ciphertext.len(),
            &fixture.payload.nonce,
            fixture.owner_wrapped,
            &fixture.content_key,
        );

        let grant = Grant::new(
            DOC_URI.to_string(),
            "did:plc:recipient".to_string(),
            fixture.recipient_wrapped,
            "2026-03-01T12:00:00Z".to_string(),
        );

        let mock = MockTransport::new();
        mock.enqueue(did_document_response());
        mock.enqueue(grant_record_response(&grant));
        mock.enqueue(record_response(&doc));
        mock.enqueue(blob_response(&fixture.payload.ciphertext));

        let (name, decrypted) = download_from_grant(&mock, &recipient_private, GRANT_URI)
            .await
            .unwrap();

        assert_eq!(name, "shared-file.txt");
        assert_eq!(decrypted, plaintext);

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 4);
        assert!(reqs[0].url.contains("plc.directory"), "DID resolution");
        assert!(reqs[1].url.contains("getRecord"), "grant fetch");
        assert!(reqs[1].url.contains(OWNER_PDS), "from owner PDS");
        assert!(reqs[2].url.contains("getRecord"), "document fetch");
        assert!(reqs[3].url.contains("getBlob"), "blob fetch");
    }

    #[tokio::test]
    async fn rejects_non_grant_uri() {
        let mock = MockTransport::new();
        let err = download_from_grant(&mock, &[0u8; 32], "at://did:plc:x/app.opake.document/abc")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("grant"), "got: {err}");
    }
}
