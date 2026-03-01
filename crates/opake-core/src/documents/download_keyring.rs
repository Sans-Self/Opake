use log::debug;

use crate::atproto;
use crate::client::{
    get_blob_public, get_record_public, pds_from_did_document, resolve_did_document, Transport,
};
use crate::crypto::{self, ContentKey, X25519PrivateKey};
use crate::error::Error;
use crate::keyrings::KEYRING_COLLECTION;
use crate::records::{self, Document, Encryption, Keyring};

use super::download::decrypt_with_nonce;
use super::DOCUMENT_COLLECTION;

/// Result of downloading a keyring-encrypted document as a member.
///
/// Includes the unwrapped group key, its rotation number, and keyring rkey so
/// the caller can cache them for subsequent downloads under the same keyring.
#[derive(Debug)]
pub struct KeyringDownloadResult {
    pub filename: String,
    pub plaintext: Vec<u8>,
    pub group_key: ContentKey,
    pub keyring_rkey: String,
    pub rotation: u64,
}

/// Download and decrypt a keyring-encrypted document as a member.
///
/// This is the cross-PDS path for keyring members: the document and keyring
/// records live on the *owner's* PDS, not the caller's. All fetches are
/// unauthenticated (public endpoints).
///
/// The caller provides a document URI on the owner's PDS. The function:
/// 1. Resolves the owner's PDS from their DID
/// 2. Fetches the document record (must be keyring-encrypted)
/// 3. Fetches the keyring record to find the member's wrapped group key
/// 4. Unwraps group key → unwraps content key → decrypts blob
pub async fn download_from_keyring_member(
    transport: &impl Transport,
    member_did: &str,
    private_key: &X25519PrivateKey,
    document_uri: &str,
) -> Result<KeyringDownloadResult, Error> {
    let doc_at = atproto::parse_at_uri(document_uri)?;
    if doc_at.collection != DOCUMENT_COLLECTION {
        return Err(Error::InvalidRecord(format!(
            "expected a document URI ({}), got collection {}",
            DOCUMENT_COLLECTION, doc_at.collection,
        )));
    }

    // Resolve the owner's PDS from their DID
    let owner_did = &doc_at.authority;
    debug!("resolving PDS for owner {}", owner_did);
    let did_doc = resolve_did_document(transport, owner_did).await?;
    let owner_pds = pds_from_did_document(&did_doc)?;

    // Fetch the document record
    debug!("fetching document from {}", owner_pds);
    let doc_entry = get_record_public(
        transport,
        &owner_pds,
        owner_did,
        DOCUMENT_COLLECTION,
        &doc_at.rkey,
    )
    .await?;

    let doc: Document = serde_json::from_value(doc_entry.value)?;
    records::check_version(doc.version)?;

    // Must be keyring-encrypted
    let kr_enc = match &doc.encryption {
        Encryption::Keyring(kr) => kr,
        Encryption::Direct(_) => {
            return Err(Error::InvalidRecord(
                "document uses direct encryption, not keyring — \
                 use `opake download` without --keyring-member"
                    .into(),
            ));
        }
    };

    // Parse keyring URI and fetch the keyring record
    let kr_at = atproto::parse_at_uri(&kr_enc.keyring_ref.keyring)?;
    debug!("fetching keyring {} from {}", kr_at.rkey, owner_pds);
    let kr_entry = get_record_public(
        transport,
        &owner_pds,
        &kr_at.authority,
        KEYRING_COLLECTION,
        &kr_at.rkey,
    )
    .await?;

    let keyring: Keyring = serde_json::from_value(kr_entry.value)?;
    records::check_version(keyring.version)?;

    // Find the member's wrapped group key — check the current rotation first,
    // then fall back to key_history if the document was encrypted under an
    // older rotation.
    let doc_rotation = kr_enc.keyring_ref.rotation;
    let member_wrapped = if doc_rotation == keyring.rotation {
        keyring.members.iter().find(|m| m.did == member_did)
    } else {
        keyring
            .key_history
            .iter()
            .find(|h| h.rotation == doc_rotation)
            .and_then(|h| h.members.iter().find(|m| m.did == member_did))
    }
    .ok_or_else(|| {
        Error::InvalidRecord(format!(
            "DID ({member_did}) is not a member of keyring {:?} at rotation {doc_rotation}",
            keyring.name,
        ))
    })?;

    // Asymmetric unwrap: member's private key → group key
    debug!("unwrapping group key for {}", member_did);
    let group_key = crypto::unwrap_key(member_wrapped, private_key)?;

    // Symmetric unwrap: group key → content key
    let wrapped_ck_bytes = kr_enc
        .keyring_ref
        .wrapped_content_key
        .decode()
        .map_err(|e| Error::InvalidRecord(format!("invalid wrapped content key: {e}")))?;
    let content_key = crypto::unwrap_content_key_from_keyring(&wrapped_ck_bytes, &group_key)?;

    // Fetch and decrypt the blob
    debug!(
        "fetching blob did={} cid={}",
        owner_did, doc.blob.reference.cid
    );
    let ciphertext =
        get_blob_public(transport, &owner_pds, owner_did, &doc.blob.reference.cid).await?;

    let plaintext = decrypt_with_nonce(&content_key, &kr_enc.nonce, ciphertext)?;

    Ok(KeyringDownloadResult {
        filename: doc.name,
        plaintext,
        group_key,
        keyring_rkey: kr_at.rkey,
        rotation: keyring.rotation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::HttpResponse;
    use crate::crypto::{OsRng, X25519DalekPublicKey, X25519DalekStaticSecret};
    use crate::records::{AtBytes, BlobRef, CidLink, KeyringEncryption, KeyringRef, WrappedKey};
    use crate::test_utils::MockTransport;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    const OWNER_DID: &str = "did:plc:owner";
    const OWNER_PDS: &str = "https://pds.owner.example.com";
    const MEMBER_DID: &str = "did:plc:member";
    const KR_RKEY: &str = "kr1";
    const DOC_URI: &str = "at://did:plc:owner/app.opake.cloud.document/doc1";
    const KR_URI: &str = "at://did:plc:owner/app.opake.cloud.keyring/kr1";

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
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn record_response(uri: &str, value: &impl serde::Serialize) -> HttpResponse {
        let body = serde_json::json!({
            "uri": uri,
            "cid": "bafyrecord",
            "value": value,
        });
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn blob_response(data: &[u8]) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: data.to_vec(),
        }
    }

    /// Encrypt plaintext under a keyring group key and wrap the group key
    /// to a member. Returns everything needed to build test fixtures.
    struct KeyringFixture {
        ciphertext: Vec<u8>,
        nonce: [u8; 12],
        group_key: ContentKey,
        owner_wrapped_gk: WrappedKey,
        member_wrapped_gk: WrappedKey,
        wrapped_content_key_bytes: Vec<u8>,
    }

    fn create_keyring_fixture(
        plaintext: &[u8],
        owner_pubkey: &[u8; 32],
        member_pubkey: &[u8; 32],
    ) -> KeyringFixture {
        let rng = &mut OsRng;

        // Generate group key and wrap to owner + member
        let group_key = crypto::generate_content_key(rng);
        let owner_wrapped_gk = crypto::wrap_key(&group_key, owner_pubkey, OWNER_DID, rng).unwrap();
        let member_wrapped_gk =
            crypto::wrap_key(&group_key, member_pubkey, MEMBER_DID, rng).unwrap();

        // Generate content key, encrypt blob, wrap CK under group key
        let content_key = crypto::generate_content_key(rng);
        let payload = crypto::encrypt_blob(&content_key, plaintext, rng).unwrap();
        let wrapped_content_key_bytes =
            crypto::wrap_content_key_for_keyring(&content_key, &group_key).unwrap();

        KeyringFixture {
            ciphertext: payload.ciphertext,
            nonce: payload.nonce,
            group_key,
            owner_wrapped_gk,
            member_wrapped_gk,
            wrapped_content_key_bytes,
        }
    }

    fn keyring_document_at_rotation(fixture: &KeyringFixture, rotation: u64) -> Document {
        Document {
            mime_type: Some("text/plain".into()),
            size: Some(42),
            ..Document::new(
                "keyring-file.txt".into(),
                BlobRef {
                    blob_type: "blob".into(),
                    reference: CidLink {
                        cid: "bafyblob".into(),
                    },
                    mime_type: "application/octet-stream".into(),
                    size: fixture.ciphertext.len() as u64,
                },
                Encryption::Keyring(KeyringEncryption {
                    keyring_ref: KeyringRef {
                        keyring: KR_URI.into(),
                        wrapped_content_key: AtBytes {
                            encoded: BASE64.encode(&fixture.wrapped_content_key_bytes),
                        },
                        rotation,
                    },
                    algo: "aes-256-gcm".into(),
                    nonce: AtBytes {
                        encoded: BASE64.encode(fixture.nonce),
                    },
                }),
                "2026-03-01T00:00:00Z".into(),
            )
        }
    }

    fn keyring_document(fixture: &KeyringFixture) -> Document {
        keyring_document_at_rotation(fixture, 0)
    }

    fn keyring_record(fixture: &KeyringFixture) -> Keyring {
        Keyring::new(
            "test-keyring".into(),
            vec![
                fixture.owner_wrapped_gk.clone(),
                fixture.member_wrapped_gk.clone(),
            ],
            "2026-03-01T00:00:00Z".into(),
        )
    }

    fn member_keypair() -> ([u8; 32], [u8; 32]) {
        let secret = X25519DalekStaticSecret::random_from_rng(OsRng);
        let public = X25519DalekPublicKey::from(&secret);
        (*public.as_bytes(), secret.to_bytes())
    }

    fn owner_keypair() -> ([u8; 32], [u8; 32]) {
        let secret = X25519DalekStaticSecret::random_from_rng(OsRng);
        let public = X25519DalekPublicKey::from(&secret);
        (*public.as_bytes(), secret.to_bytes())
    }

    #[tokio::test]
    async fn roundtrip() {
        let (owner_pub, _) = owner_keypair();
        let (member_pub, member_priv) = member_keypair();

        let plaintext = b"shared keyring content";
        let fixture = create_keyring_fixture(plaintext, &owner_pub, &member_pub);
        let doc = keyring_document(&fixture);
        let keyring = keyring_record(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(did_document_response());
        mock.enqueue(record_response(DOC_URI, &doc));
        mock.enqueue(record_response(KR_URI, &keyring));
        mock.enqueue(blob_response(&fixture.ciphertext));

        let result = download_from_keyring_member(&mock, MEMBER_DID, &member_priv, DOC_URI)
            .await
            .unwrap();

        assert_eq!(result.filename, "keyring-file.txt");
        assert_eq!(result.plaintext, plaintext);
        assert_eq!(result.keyring_rkey, KR_RKEY);

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 4);
        assert!(reqs[0].url.contains("plc.directory"), "DID resolution");
        assert!(reqs[1].url.contains("getRecord"), "document fetch");
        assert!(reqs[1].url.contains(OWNER_PDS), "from owner PDS");
        assert!(reqs[2].url.contains("getRecord"), "keyring fetch");
        assert!(reqs[3].url.contains("getBlob"), "blob fetch");
    }

    #[tokio::test]
    async fn rejects_non_document_uri() {
        let mock = MockTransport::new();
        let err = download_from_keyring_member(
            &mock,
            MEMBER_DID,
            &[0u8; 32],
            "at://did:plc:x/app.opake.cloud.grant/abc",
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("document"),
            "expected document error, got: {err}"
        );
    }

    #[tokio::test]
    async fn rejects_direct_encrypted_document() {
        let (member_pub, member_priv) = member_keypair();

        // Build a direct-encrypted document (not keyring)
        let content_key = crypto::generate_content_key(&mut OsRng);
        let payload = crypto::encrypt_blob(&content_key, b"data", &mut OsRng).unwrap();
        let wrapped = crypto::wrap_key(&content_key, &member_pub, MEMBER_DID, &mut OsRng).unwrap();

        let doc = Document {
            mime_type: Some("text/plain".into()),
            size: Some(4),
            ..Document::new(
                "direct-file.txt".into(),
                BlobRef {
                    blob_type: "blob".into(),
                    reference: CidLink {
                        cid: "bafyblob".into(),
                    },
                    mime_type: "application/octet-stream".into(),
                    size: payload.ciphertext.len() as u64,
                },
                Encryption::Direct(records::DirectEncryption {
                    envelope: records::EncryptionEnvelope {
                        algo: "aes-256-gcm".into(),
                        nonce: AtBytes {
                            encoded: BASE64.encode(payload.nonce),
                        },
                        keys: vec![wrapped],
                    },
                }),
                "2026-03-01T00:00:00Z".into(),
            )
        };

        let mock = MockTransport::new();
        mock.enqueue(did_document_response());
        mock.enqueue(record_response(DOC_URI, &doc));

        let err = download_from_keyring_member(&mock, MEMBER_DID, &member_priv, DOC_URI)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("direct encryption"),
            "expected direct encryption error, got: {err}"
        );
    }

    #[tokio::test]
    async fn rejects_non_member() {
        let (owner_pub, _) = owner_keypair();
        let (member_pub, _) = member_keypair();
        let (_, outsider_priv) = member_keypair();

        let fixture = create_keyring_fixture(b"secret", &owner_pub, &member_pub);
        let doc = keyring_document(&fixture);
        let keyring = keyring_record(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(did_document_response());
        mock.enqueue(record_response(DOC_URI, &doc));
        mock.enqueue(record_response(KR_URI, &keyring));

        let err = download_from_keyring_member(&mock, "did:plc:outsider", &outsider_priv, DOC_URI)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("not a member"),
            "expected member error, got: {err}"
        );
    }

    #[tokio::test]
    async fn returns_group_key_for_caching() {
        let (owner_pub, _) = owner_keypair();
        let (member_pub, member_priv) = member_keypair();

        let plaintext = b"cache test content";
        let fixture = create_keyring_fixture(plaintext, &owner_pub, &member_pub);

        // Save the original group key bytes for comparison
        let original_gk_bytes = fixture.group_key.0;

        let doc = keyring_document(&fixture);
        let keyring = keyring_record(&fixture);

        let mock = MockTransport::new();
        mock.enqueue(did_document_response());
        mock.enqueue(record_response(DOC_URI, &doc));
        mock.enqueue(record_response(KR_URI, &keyring));
        mock.enqueue(blob_response(&fixture.ciphertext));

        let result = download_from_keyring_member(&mock, MEMBER_DID, &member_priv, DOC_URI)
            .await
            .unwrap();

        // The returned group key should match what was used to wrap content keys
        assert_eq!(result.group_key.0, original_gk_bytes);

        // Verify the group key can unwrap the content key independently
        let ck = crypto::unwrap_content_key_from_keyring(
            &fixture.wrapped_content_key_bytes,
            &result.group_key,
        )
        .expect("cached group key should unwrap content key");

        // Re-decrypt using the independently unwrapped content key
        let re_decrypted = crypto::decrypt_blob(
            &ck,
            &crypto::EncryptedPayload {
                ciphertext: fixture.ciphertext.clone(),
                nonce: fixture.nonce,
            },
        )
        .unwrap();
        assert_eq!(re_decrypted, plaintext);
    }

    #[tokio::test]
    async fn download_from_previous_rotation_via_history() {
        let (owner_pub, _) = owner_keypair();
        let (member_pub, member_priv) = member_keypair();

        let plaintext = b"pre-rotation content";
        let fixture = create_keyring_fixture(plaintext, &owner_pub, &member_pub);

        // Document was uploaded at rotation 0
        let doc = keyring_document_at_rotation(&fixture, 0);

        // Keyring has since rotated to 1 — rotation 0 members are in key_history
        let mut keyring = Keyring {
            rotation: 1,
            members: vec![fixture.owner_wrapped_gk.clone()],
            key_history: vec![records::KeyHistoryEntry {
                rotation: 0,
                members: vec![
                    fixture.owner_wrapped_gk.clone(),
                    fixture.member_wrapped_gk.clone(),
                ],
            }],
            ..keyring_record(&fixture)
        };
        // Suppress the members from new() since we overwrote them
        let _ = &mut keyring;

        let mock = MockTransport::new();
        mock.enqueue(did_document_response());
        mock.enqueue(record_response(DOC_URI, &doc));
        mock.enqueue(record_response(KR_URI, &keyring));
        mock.enqueue(blob_response(&fixture.ciphertext));

        let result = download_from_keyring_member(&mock, MEMBER_DID, &member_priv, DOC_URI)
            .await
            .unwrap();

        assert_eq!(result.plaintext, plaintext);
        assert_eq!(result.rotation, 1); // returns current keyring rotation for caching
    }

    #[tokio::test]
    async fn rejects_member_not_present_at_historical_rotation() {
        let (owner_pub, _) = owner_keypair();
        let (member_pub, _) = member_keypair();
        let (_, outsider_priv) = member_keypair();

        let fixture = create_keyring_fixture(b"data", &owner_pub, &member_pub);

        // Document encrypted at rotation 0
        let doc = keyring_document_at_rotation(&fixture, 0);

        // Keyring is at rotation 1, history has rotation 0 with only owner
        let keyring = Keyring {
            rotation: 1,
            members: vec![fixture.owner_wrapped_gk.clone()],
            key_history: vec![records::KeyHistoryEntry {
                rotation: 0,
                members: vec![fixture.owner_wrapped_gk.clone()],
            }],
            ..keyring_record(&fixture)
        };

        let mock = MockTransport::new();
        mock.enqueue(did_document_response());
        mock.enqueue(record_response(DOC_URI, &doc));
        mock.enqueue(record_response(KR_URI, &keyring));

        let err = download_from_keyring_member(&mock, "did:plc:outsider", &outsider_priv, DOC_URI)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("not a member"),
            "expected member error, got: {err}"
        );
    }
}
