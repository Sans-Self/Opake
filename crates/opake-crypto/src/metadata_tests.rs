use super::*;
use crate::{generate_content_key, OsRng, SealContext, SealType};

const TEST_URI: &str = "at://did:plc:test/at.opake.document/3seal";

fn seal(seal_type: SealType) -> SealContext<'static> {
    SealContext::new(TEST_URI, seal_type)
}

fn sample_metadata() -> DocumentMetadata {
    DocumentMetadata {
        name: "tax-return-2025.pdf".into(),
        mime_type: Some("application/pdf".into()),
        size: Some(1_048_576),
        tags: vec!["finance".into(), "2025".into()],
        description: Some("Annual tax return".into()),
    }
}

#[test]
fn roundtrip() {
    let key = generate_content_key(&mut OsRng);
    let metadata = sample_metadata();

    let encrypted = encrypt_metadata(
        &key,
        &metadata,
        &seal(SealType::DocumentMetadata),
        &mut OsRng,
    )
    .unwrap();
    let decrypted: DocumentMetadata =
        decrypt_metadata(&key, &encrypted, &seal(SealType::DocumentMetadata)).unwrap();

    assert_eq!(decrypted, metadata);
}

#[test]
fn wrong_key_fails() {
    let key = generate_content_key(&mut OsRng);
    let wrong_key = generate_content_key(&mut OsRng);
    let metadata = sample_metadata();

    let encrypted = encrypt_metadata(
        &key,
        &metadata,
        &seal(SealType::DocumentMetadata),
        &mut OsRng,
    )
    .unwrap();
    let err = decrypt_metadata::<DocumentMetadata>(
        &wrong_key,
        &encrypted,
        &seal(SealType::DocumentMetadata),
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("aead"),
        "expected decryption error, got: {err}"
    );
}

#[test]
fn minimal_metadata() {
    let key = generate_content_key(&mut OsRng);
    let metadata = DocumentMetadata {
        name: "unnamed".into(),
        mime_type: None,
        size: None,
        tags: vec![],
        description: None,
    };

    let encrypted = encrypt_metadata(
        &key,
        &metadata,
        &seal(SealType::DocumentMetadata),
        &mut OsRng,
    )
    .unwrap();
    let decrypted: DocumentMetadata =
        decrypt_metadata(&key, &encrypted, &seal(SealType::DocumentMetadata)).unwrap();

    assert_eq!(decrypted.name, "unnamed");
    assert!(decrypted.mime_type.is_none());
    assert!(decrypted.size.is_none());
    assert!(decrypted.tags.is_empty());
    assert!(decrypted.description.is_none());
}

#[test]
fn different_nonces_per_encryption() {
    let key = generate_content_key(&mut OsRng);
    let metadata = sample_metadata();

    let a = encrypt_metadata(
        &key,
        &metadata,
        &seal(SealType::DocumentMetadata),
        &mut OsRng,
    )
    .unwrap();
    let b = encrypt_metadata(
        &key,
        &metadata,
        &seal(SealType::DocumentMetadata),
        &mut OsRng,
    )
    .unwrap();

    assert_ne!(a.nonce.encoded, b.nonce.encoded);
    assert_ne!(a.ciphertext.encoded, b.ciphertext.encoded);
}

#[test]
fn camel_case_serialization() {
    let metadata = sample_metadata();
    let json = serde_json::to_value(&metadata).unwrap();

    assert!(json.get("mimeType").is_some());
    assert!(json.get("mime_type").is_none());
}

#[test]
fn keyring_metadata_roundtrip() {
    let key = generate_content_key(&mut OsRng);
    let metadata = KeyringMetadata {
        name: "family-photos".into(),
        description: Some("Photos from the holidays".into()),
        icon: None,
    };

    let encrypted = encrypt_metadata(
        &key,
        &metadata,
        &seal(SealType::DocumentMetadata),
        &mut OsRng,
    )
    .unwrap();
    let decrypted: KeyringMetadata =
        decrypt_metadata(&key, &encrypted, &seal(SealType::DocumentMetadata)).unwrap();

    assert_eq!(decrypted.name, "family-photos");
    assert_eq!(
        decrypted.description.as_deref(),
        Some("Photos from the holidays")
    );
}

#[test]
fn keyring_metadata_minimal() {
    let key = generate_content_key(&mut OsRng);
    let metadata = KeyringMetadata {
        name: "bare".into(),
        description: None,
        icon: None,
    };

    let encrypted = encrypt_metadata(
        &key,
        &metadata,
        &seal(SealType::DocumentMetadata),
        &mut OsRng,
    )
    .unwrap();
    let decrypted: KeyringMetadata =
        decrypt_metadata(&key, &encrypted, &seal(SealType::DocumentMetadata)).unwrap();

    assert_eq!(decrypted.name, "bare");
    assert!(decrypted.description.is_none());
}

#[test]
fn grant_metadata_roundtrip() {
    let key = generate_content_key(&mut OsRng);
    let metadata = GrantMetadata {
        permissions: Some("read".into()),
        note: Some("shared for review".into()),
        unverified_key_approval: Some([7; 32]),
        pending_share_uri: None,
        pending_share_commitment: None,
    };

    let encrypted = encrypt_metadata(
        &key,
        &metadata,
        &seal(SealType::DocumentMetadata),
        &mut OsRng,
    )
    .unwrap();
    let decrypted: GrantMetadata =
        decrypt_metadata(&key, &encrypted, &seal(SealType::DocumentMetadata)).unwrap();

    assert_eq!(decrypted.permissions.as_deref(), Some("read"));
    assert_eq!(decrypted.note.as_deref(), Some("shared for review"));
    assert_eq!(decrypted.unverified_key_approval, Some([7; 32]));
}

#[test]
fn grant_metadata_minimal() {
    let key = generate_content_key(&mut OsRng);
    let metadata = GrantMetadata {
        permissions: None,
        note: None,
        unverified_key_approval: None,
        pending_share_uri: None,
        pending_share_commitment: None,
    };

    let encrypted = encrypt_metadata(
        &key,
        &metadata,
        &seal(SealType::DocumentMetadata),
        &mut OsRng,
    )
    .unwrap();
    let decrypted: GrantMetadata =
        decrypt_metadata(&key, &encrypted, &seal(SealType::DocumentMetadata)).unwrap();

    assert!(decrypted.permissions.is_none());
    assert!(decrypted.note.is_none());
    assert!(decrypted.unverified_key_approval.is_none());
}

#[test]
fn directory_metadata_roundtrip() {
    let key = generate_content_key(&mut OsRng);
    let metadata = DirectoryMetadata {
        name: "Photos".into(),
        description: Some("Vacation photos".into()),
    };

    let encrypted = encrypt_metadata(
        &key,
        &metadata,
        &seal(SealType::DocumentMetadata),
        &mut OsRng,
    )
    .unwrap();
    let decrypted: DirectoryMetadata =
        decrypt_metadata(&key, &encrypted, &seal(SealType::DocumentMetadata)).unwrap();

    assert_eq!(decrypted.name, "Photos");
    assert_eq!(decrypted.description.as_deref(), Some("Vacation photos"));
}

#[test]
fn directory_metadata_minimal() {
    let key = generate_content_key(&mut OsRng);
    let metadata = DirectoryMetadata {
        name: "/".into(),
        description: None,
    };

    let encrypted = encrypt_metadata(
        &key,
        &metadata,
        &seal(SealType::DocumentMetadata),
        &mut OsRng,
    )
    .unwrap();
    let decrypted: DirectoryMetadata =
        decrypt_metadata(&key, &encrypted, &seal(SealType::DocumentMetadata)).unwrap();

    assert_eq!(decrypted.name, "/");
    assert!(decrypted.description.is_none());
}

#[test]
fn pending_share_metadata_roundtrips_bound_did_and_explicit_permission() {
    let key = generate_content_key(&mut OsRng);
    let metadata = PendingShareMetadata {
        permissions: Some("read".into()),
        note: Some("for review".into()),
        recipient_did: "did:plc:recipient".into(),
        allow_unverified_first_publication: true,
    };
    let encrypted = encrypt_metadata(
        &key,
        &metadata,
        &seal(SealType::PendingShareMetadata),
        &mut OsRng,
    )
    .unwrap();

    let decrypted: PendingShareMetadata =
        decrypt_metadata(&key, &encrypted, &seal(SealType::PendingShareMetadata)).unwrap();
    assert_eq!(decrypted, metadata);

    let replay_as_grant: Result<GrantMetadata, _> =
        decrypt_metadata(&key, &encrypted, &seal(SealType::GrantMetadata));
    assert!(
        replay_as_grant.is_err(),
        "a pending intent must not authenticate as a completed grant"
    );
}

#[test]
fn pending_share_metadata_missing_permission_cannot_default_to_consent() {
    let legacy = serde_json::json!({
        "permissions": "read",
        "recipientDid": "did:plc:recipient"
    });

    assert!(serde_json::from_value::<PendingShareMetadata>(legacy).is_err());
}

// spec: document-crypto § Ciphertexts are AAD-bound to their lineage anchor and type
// (scenario: a blob ciphertext pasted into the metadata slot fails authentication)
#[test]
#[allow(non_snake_case)] // bug__ regression-naming convention
fn bug__blob_metadata_swap_under_shared_key_fails_authentication() {
    use crate::{decrypt_blob, encrypt_blob};
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    // One content key seals both a document's blob and its metadata. Before
    // AAD, swapping the ciphertexts decrypted cleanly and failed only if the
    // bytes didn't parse. The seal type must reject the swap at the AEAD.
    let key = generate_content_key(&mut OsRng);

    let blob_payload = encrypt_blob(
        &key,
        b"file bytes",
        &seal(SealType::DocumentBlob),
        &mut OsRng,
    )
    .unwrap();
    let encrypted_metadata = encrypt_metadata(
        &key,
        &sample_metadata(),
        &seal(SealType::DocumentMetadata),
        &mut OsRng,
    )
    .unwrap();

    // Blob ciphertext presented in the metadata slot.
    let blob_as_metadata = EncryptedMetadata {
        ciphertext: AtBytes {
            encoded: BASE64.encode(&blob_payload.ciphertext),
        },
        nonce: AtBytes {
            encoded: BASE64.encode(blob_payload.nonce),
        },
    };
    assert!(decrypt_metadata::<DocumentMetadata>(
        &key,
        &blob_as_metadata,
        &seal(SealType::DocumentMetadata)
    )
    .is_err());

    // Metadata ciphertext presented in the blob slot.
    let metadata_as_blob = crate::EncryptedPayload {
        ciphertext: encrypted_metadata.ciphertext.decode().unwrap(),
        nonce: encrypted_metadata
            .nonce
            .decode()
            .unwrap()
            .try_into()
            .unwrap(),
    };
    assert!(decrypt_blob(&key, &metadata_as_blob, &seal(SealType::DocumentBlob)).is_err());
}

#[test]
#[allow(non_snake_case)]
fn bug__wrong_length_nonce_panicked_instead_of_erroring() {
    let key = generate_content_key(&mut OsRng);
    let mut encrypted = encrypt_metadata(
        &key,
        &sample_metadata(),
        &seal(SealType::DocumentMetadata),
        &mut OsRng,
    )
    .unwrap();
    // A served record is untrusted input: a truncated nonce must surface as a
    // decryption error, never unwind the caller (fatal in WASM).
    encrypted.nonce = crate::AtBytes::from_raw(&[1, 2, 3]);
    let err =
        decrypt_metadata::<DocumentMetadata>(&key, &encrypted, &seal(SealType::DocumentMetadata))
            .unwrap_err();
    assert!(
        err.to_string().contains("nonce length"),
        "expected a nonce-length error, got: {err}"
    );
}
