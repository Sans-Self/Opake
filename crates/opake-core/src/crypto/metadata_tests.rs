use super::*;
use crate::crypto::{generate_content_key, OsRng};

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

    let encrypted = encrypt_metadata(&key, &metadata, &mut OsRng).unwrap();
    let decrypted: DocumentMetadata = decrypt_metadata(&key, &encrypted).unwrap();

    assert_eq!(decrypted, metadata);
}

#[test]
fn wrong_key_fails() {
    let key = generate_content_key(&mut OsRng);
    let wrong_key = generate_content_key(&mut OsRng);
    let metadata = sample_metadata();

    let encrypted = encrypt_metadata(&key, &metadata, &mut OsRng).unwrap();
    let err = decrypt_metadata::<DocumentMetadata>(&wrong_key, &encrypted).unwrap_err();

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

    let encrypted = encrypt_metadata(&key, &metadata, &mut OsRng).unwrap();
    let decrypted: DocumentMetadata = decrypt_metadata(&key, &encrypted).unwrap();

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

    let a = encrypt_metadata(&key, &metadata, &mut OsRng).unwrap();
    let b = encrypt_metadata(&key, &metadata, &mut OsRng).unwrap();

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
        enforce_revocation: None,
    };

    let encrypted = encrypt_metadata(&key, &metadata, &mut OsRng).unwrap();
    let decrypted: KeyringMetadata = decrypt_metadata(&key, &encrypted).unwrap();

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
        enforce_revocation: None,
    };

    let encrypted = encrypt_metadata(&key, &metadata, &mut OsRng).unwrap();
    let decrypted: KeyringMetadata = decrypt_metadata(&key, &encrypted).unwrap();

    assert_eq!(decrypted.name, "bare");
    assert!(decrypted.description.is_none());
}

#[test]
fn grant_metadata_roundtrip() {
    let key = generate_content_key(&mut OsRng);
    let metadata = GrantMetadata {
        permissions: Some("read".into()),
        note: Some("shared for review".into()),
    };

    let encrypted = encrypt_metadata(&key, &metadata, &mut OsRng).unwrap();
    let decrypted: GrantMetadata = decrypt_metadata(&key, &encrypted).unwrap();

    assert_eq!(decrypted.permissions.as_deref(), Some("read"));
    assert_eq!(decrypted.note.as_deref(), Some("shared for review"));
}

#[test]
fn grant_metadata_minimal() {
    let key = generate_content_key(&mut OsRng);
    let metadata = GrantMetadata {
        permissions: None,
        note: None,
    };

    let encrypted = encrypt_metadata(&key, &metadata, &mut OsRng).unwrap();
    let decrypted: GrantMetadata = decrypt_metadata(&key, &encrypted).unwrap();

    assert!(decrypted.permissions.is_none());
    assert!(decrypted.note.is_none());
}

#[test]
fn directory_metadata_roundtrip() {
    let key = generate_content_key(&mut OsRng);
    let metadata = DirectoryMetadata {
        name: "Photos".into(),
        description: Some("Vacation photos".into()),
    };

    let encrypted = encrypt_metadata(&key, &metadata, &mut OsRng).unwrap();
    let decrypted: DirectoryMetadata = decrypt_metadata(&key, &encrypted).unwrap();

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

    let encrypted = encrypt_metadata(&key, &metadata, &mut OsRng).unwrap();
    let decrypted: DirectoryMetadata = decrypt_metadata(&key, &encrypted).unwrap();

    assert_eq!(decrypted.name, "/");
    assert!(decrypted.description.is_none());
}
