use super::*;
use aes_gcm::aead::rand_core::OsRng;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use x25519_dalek::{PublicKey, StaticSecret};

// -- Content encryption tests (AES-256-GCM) --

#[test]
fn roundtrip_encrypt_decrypt() {
    let key = generate_content_key(&mut OsRng);
    let plaintext = b"hello opake";
    let payload = encrypt_blob(&key, plaintext, &mut OsRng).unwrap();
    let decrypted = decrypt_blob(&key, &payload).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn wrong_key_fails_decryption() {
    let key = generate_content_key(&mut OsRng);
    let wrong_key = generate_content_key(&mut OsRng);
    let payload = encrypt_blob(&key, b"secret", &mut OsRng).unwrap();
    assert!(decrypt_blob(&wrong_key, &payload).is_err());
}

#[test]
fn empty_plaintext_roundtrips() {
    let key = generate_content_key(&mut OsRng);
    let payload = encrypt_blob(&key, b"", &mut OsRng).unwrap();
    let decrypted = decrypt_blob(&key, &payload).unwrap();
    assert!(decrypted.is_empty());
}

#[test]
fn ciphertext_differs_from_plaintext() {
    let key = generate_content_key(&mut OsRng);
    let plaintext = b"not encrypted i promise";
    let payload = encrypt_blob(&key, plaintext, &mut OsRng).unwrap();
    assert_ne!(payload.ciphertext, plaintext);
}

#[test]
fn unique_nonces_per_encryption() {
    let key = generate_content_key(&mut OsRng);
    let a = encrypt_blob(&key, b"same", &mut OsRng).unwrap();
    let b = encrypt_blob(&key, b"same", &mut OsRng).unwrap();
    assert_ne!(a.nonce, b.nonce);
}

#[test]
fn tampered_ciphertext_fails() {
    let key = generate_content_key(&mut OsRng);
    let mut payload = encrypt_blob(&key, b"integrity", &mut OsRng).unwrap();
    payload.ciphertext[0] ^= 0xff;
    assert!(decrypt_blob(&key, &payload).is_err());
}

#[test]
fn large_payload_roundtrips() {
    let key = generate_content_key(&mut OsRng);
    let plaintext = vec![0xAB_u8; 1_000_000];
    let payload = encrypt_blob(&key, &plaintext, &mut OsRng).unwrap();
    let decrypted = decrypt_blob(&key, &payload).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn tampered_nonce_fails() {
    let key = generate_content_key(&mut OsRng);
    let mut payload = encrypt_blob(&key, b"nonce matters", &mut OsRng).unwrap();
    payload.nonce[0] ^= 0xff;
    assert!(decrypt_blob(&key, &payload).is_err());
}

// -- Key wrapping tests (x25519-hkdf-a256kw) --

fn test_keypair() -> (StaticSecret, PublicKey) {
    let private = StaticSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&private);
    (private, public)
}

#[test]
fn wrap_unwrap_roundtrips() {
    let content_key = generate_content_key(&mut OsRng);
    let (private, public) = test_keypair();

    let wrapped = wrap_key(&content_key, public.as_bytes(), "did:plc:test", &mut OsRng).unwrap();
    let unwrapped = unwrap_key(&wrapped, &private.to_bytes()).unwrap();

    assert_eq!(content_key.0, unwrapped.0);
}

#[test]
fn wrap_produces_correct_algo() {
    let content_key = generate_content_key(&mut OsRng);
    let (_private, public) = test_keypair();

    let wrapped = wrap_key(&content_key, public.as_bytes(), "did:plc:test", &mut OsRng).unwrap();
    assert_eq!(wrapped.algo, "x25519-hkdf-a256kw");
    assert_eq!(wrapped.did, "did:plc:test");
}

#[test]
fn wrap_ciphertext_is_expected_length() {
    let content_key = generate_content_key(&mut OsRng);
    let (_private, public) = test_keypair();

    let wrapped = wrap_key(&content_key, public.as_bytes(), "did:plc:test", &mut OsRng).unwrap();
    let decoded = BASE64.decode(&wrapped.ciphertext.encoded).unwrap();
    assert_eq!(decoded.len(), CIPHERTEXT_LEN);
}

#[test]
fn wrong_private_key_fails_unwrap() {
    let content_key = generate_content_key(&mut OsRng);
    let (_private, public) = test_keypair();
    let (wrong_private, _) = test_keypair();

    let wrapped = wrap_key(&content_key, public.as_bytes(), "did:plc:test", &mut OsRng).unwrap();
    assert!(unwrap_key(&wrapped, &wrong_private.to_bytes()).is_err());
}

#[test]
fn tampered_wrapped_ciphertext_fails_unwrap() {
    let content_key = generate_content_key(&mut OsRng);
    let (private, public) = test_keypair();

    let mut wrapped =
        wrap_key(&content_key, public.as_bytes(), "did:plc:test", &mut OsRng).unwrap();
    let mut bytes = BASE64.decode(&wrapped.ciphertext.encoded).unwrap();
    bytes[40] ^= 0xff;
    wrapped.ciphertext.encoded = BASE64.encode(&bytes);

    assert!(unwrap_key(&wrapped, &private.to_bytes()).is_err());
}

#[test]
fn each_wrap_produces_unique_ciphertext() {
    let content_key = generate_content_key(&mut OsRng);
    let (_private, public) = test_keypair();

    let a = wrap_key(&content_key, public.as_bytes(), "did:plc:test", &mut OsRng).unwrap();
    let b = wrap_key(&content_key, public.as_bytes(), "did:plc:test", &mut OsRng).unwrap();
    assert_ne!(a.ciphertext.encoded, b.ciphertext.encoded);
}

#[test]
fn create_group_key_wraps_to_all_members() {
    let (priv_a, pub_a) = test_keypair();
    let (priv_b, pub_b) = test_keypair();

    let members = vec![
        DidMember {
            did: "did:plc:alice",
            public_key: pub_a.as_bytes(),
        },
        DidMember {
            did: "did:plc:bob",
            public_key: pub_b.as_bytes(),
        },
    ];

    let (group_key, wrapped_keys) = create_group_key(&members, &mut OsRng).unwrap();
    assert_eq!(wrapped_keys.len(), 2);
    assert_eq!(wrapped_keys[0].did, "did:plc:alice");
    assert_eq!(wrapped_keys[1].did, "did:plc:bob");

    let unwrapped_a = unwrap_key(&wrapped_keys[0], &priv_a.to_bytes()).unwrap();
    let unwrapped_b = unwrap_key(&wrapped_keys[1], &priv_b.to_bytes()).unwrap();
    assert_eq!(group_key.0, unwrapped_a.0);
    assert_eq!(group_key.0, unwrapped_b.0);
}

// -- Ephemeral keypair --

#[test]
fn ephemeral_keypair_has_correct_key_lengths() {
    let kp = generate_ephemeral_keypair(&mut OsRng);
    assert_eq!(kp.public_key.len(), 32);
    assert_eq!(kp.private_key.len(), 32);
}

#[test]
fn ephemeral_keypair_unique_each_time() {
    let a = generate_ephemeral_keypair(&mut OsRng);
    let b = generate_ephemeral_keypair(&mut OsRng);
    assert_ne!(a.public_key, b.public_key);
    assert_ne!(a.private_key, b.private_key);
}

#[test]
fn ephemeral_keypair_compatible_with_wrap_unwrap() {
    let ephemeral = generate_ephemeral_keypair(&mut OsRng);
    let content_key = generate_content_key(&mut OsRng);

    let wrapped = wrap_key(
        &content_key,
        &ephemeral.public_key,
        "did:plc:ephemeral",
        &mut OsRng,
    )
    .unwrap();
    let unwrapped = unwrap_key(&wrapped, &ephemeral.private_key).unwrap();

    assert_eq!(content_key.0, unwrapped.0);
}

// -- Keyring wrapping (symmetric AES-KW) --

#[test]
fn keyring_content_key_roundtrips() {
    let group_key = generate_content_key(&mut OsRng);
    let content_key = generate_content_key(&mut OsRng);
    let wrapped = wrap_content_key_for_keyring(&content_key, &group_key).unwrap();
    let unwrapped = unwrap_content_key_from_keyring(&wrapped, &group_key).unwrap();
    assert_eq!(content_key.0, unwrapped.0);
}

#[test]
fn keyring_wrapped_key_is_expected_length() {
    let group_key = generate_content_key(&mut OsRng);
    let content_key = generate_content_key(&mut OsRng);
    let wrapped = wrap_content_key_for_keyring(&content_key, &group_key).unwrap();
    assert_eq!(wrapped.len(), WRAPPED_KEY_LEN);
}

#[test]
fn keyring_wrong_group_key_fails() {
    let group_key = generate_content_key(&mut OsRng);
    let wrong_key = generate_content_key(&mut OsRng);
    let content_key = generate_content_key(&mut OsRng);
    let wrapped = wrap_content_key_for_keyring(&content_key, &group_key).unwrap();
    assert!(unwrap_content_key_from_keyring(&wrapped, &wrong_key).is_err());
}

#[test]
fn keyring_wrong_length_input_fails() {
    let group_key = generate_content_key(&mut OsRng);
    assert!(unwrap_content_key_from_keyring(&[0u8; 10], &group_key).is_err());
    assert!(unwrap_content_key_from_keyring(&[0u8; 64], &group_key).is_err());
    assert!(unwrap_content_key_from_keyring(&[], &group_key).is_err());
}
