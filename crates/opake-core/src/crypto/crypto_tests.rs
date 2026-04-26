use super::*;
use aes_gcm::aead::rand_core::OsRng;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use libcrux_ml_kem::mlkem768;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

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

// -- Hybrid key wrapping tests (x25519-mlkem768-hkdf-a256kw) --

/// Owned hybrid keypair for crypto-level tests.
///
/// Local helper duplicating the structure of `test_utils::TestKeys` because
/// crypto_tests is in the same module as the implementation it exercises;
/// pulling in the workspace-wide test helper would add a circular module
/// dependency for what amounts to four byte arrays.
struct LocalKeys {
    x25519_pub: X25519PublicKey,
    x25519_priv: Zeroizing<X25519PrivateKey>,
    ml_kem_pub: MlKemPublicKey,
    ml_kem_priv: Zeroizing<MlKemPrivateKey>,
}

impl LocalKeys {
    fn generate() -> Self {
        let x25519_priv = StaticSecret::random_from_rng(OsRng);
        let x25519_pub = PublicKey::from(&x25519_priv);

        let mut mlkem_seed = [0u8; ML_KEM_KEYGEN_RANDOMNESS_LEN];
        let mut rng = OsRng;
        rng.fill_bytes(&mut mlkem_seed);
        let kp = mlkem768::generate_key_pair(mlkem_seed);
        let ml_kem_pub: [u8; ML_KEM_PK_LEN] = (*kp.public_key().as_ref())
            .try_into()
            .expect("ML-KEM-768 public key must be 1184 bytes");
        let ml_kem_priv: [u8; ML_KEM_SK_LEN] = (*kp.private_key().as_ref())
            .try_into()
            .expect("ML-KEM-768 private key must be 2400 bytes");

        Self {
            x25519_pub: *x25519_pub.as_bytes(),
            x25519_priv: Zeroizing::new(x25519_priv.to_bytes()),
            ml_kem_pub,
            ml_kem_priv: Zeroizing::new(ml_kem_priv),
        }
    }

    fn public_keys(&self) -> PublicKeyBundle<'_> {
        PublicKeyBundle {
            x25519: &self.x25519_pub,
            ml_kem: &self.ml_kem_pub,
        }
    }

    fn private_keys(&self) -> PrivateKeyBundle<'_> {
        PrivateKeyBundle {
            x25519: &self.x25519_priv,
            ml_kem: &self.ml_kem_priv,
        }
    }
}

#[test]
fn wrap_unwrap_roundtrips() {
    let content_key = generate_content_key(&mut OsRng);
    let keys = LocalKeys::generate();

    let wrapped = wrap_key(
        &content_key,
        &keys.public_keys(),
        "did:plc:test",
        &mut OsRng,
    )
    .unwrap();
    let unwrapped = unwrap_key(&wrapped, &keys.private_keys()).unwrap();

    assert_eq!(content_key.0, unwrapped.0);
}

#[test]
fn wrap_produces_correct_algo() {
    let content_key = generate_content_key(&mut OsRng);
    let keys = LocalKeys::generate();

    let wrapped = wrap_key(
        &content_key,
        &keys.public_keys(),
        "did:plc:test",
        &mut OsRng,
    )
    .unwrap();
    assert_eq!(wrapped.algo, HYBRID_WRAP_ALGO);
    assert_eq!(wrapped.did, "did:plc:test");
}

#[test]
fn wrap_ciphertext_is_expected_length() {
    let content_key = generate_content_key(&mut OsRng);
    let keys = LocalKeys::generate();

    let wrapped = wrap_key(
        &content_key,
        &keys.public_keys(),
        "did:plc:test",
        &mut OsRng,
    )
    .unwrap();
    let decoded = BASE64.decode(&wrapped.ciphertext.encoded).unwrap();
    assert_eq!(decoded.len(), HYBRID_CIPHERTEXT_LEN);
    // Sanity-check the layout against the documented byte ranges.
    assert_eq!(HYBRID_CIPHERTEXT_LEN, 32 + 1088 + 40);
}

#[test]
fn wrong_private_key_fails_unwrap() {
    let content_key = generate_content_key(&mut OsRng);
    let keys = LocalKeys::generate();
    let wrong_keys = LocalKeys::generate();

    let wrapped = wrap_key(
        &content_key,
        &keys.public_keys(),
        "did:plc:test",
        &mut OsRng,
    )
    .unwrap();
    assert!(unwrap_key(&wrapped, &wrong_keys.private_keys()).is_err());
}

#[test]
fn tampered_wrapped_ciphertext_fails_unwrap() {
    let content_key = generate_content_key(&mut OsRng);
    let keys = LocalKeys::generate();

    let mut wrapped = wrap_key(
        &content_key,
        &keys.public_keys(),
        "did:plc:test",
        &mut OsRng,
    )
    .unwrap();
    let mut bytes = BASE64.decode(&wrapped.ciphertext.encoded).unwrap();
    // Flip a byte inside the AES-KW wrapped portion (after eph_pub + ml_kem_ct).
    let last_byte_index = bytes.len() - 1;
    bytes[last_byte_index] ^= 0xff;
    wrapped.ciphertext.encoded = BASE64.encode(&bytes);

    assert!(unwrap_key(&wrapped, &keys.private_keys()).is_err());
}

#[test]
fn each_wrap_produces_unique_ciphertext() {
    let content_key = generate_content_key(&mut OsRng);
    let keys = LocalKeys::generate();

    let a = wrap_key(
        &content_key,
        &keys.public_keys(),
        "did:plc:test",
        &mut OsRng,
    )
    .unwrap();
    let b = wrap_key(
        &content_key,
        &keys.public_keys(),
        "did:plc:test",
        &mut OsRng,
    )
    .unwrap();
    assert_ne!(a.ciphertext.encoded, b.ciphertext.encoded);
}

#[test]
fn create_group_key_wraps_to_all_members() {
    let alice = LocalKeys::generate();
    let bob = LocalKeys::generate();

    let members = vec![
        DidMember {
            did: "did:plc:alice",
            x25519_public_key: &alice.x25519_pub,
            ml_kem_public_key: &alice.ml_kem_pub,
        },
        DidMember {
            did: "did:plc:bob",
            x25519_public_key: &bob.x25519_pub,
            ml_kem_public_key: &bob.ml_kem_pub,
        },
    ];

    let (group_key, wrapped_keys) = create_group_key(&members, &mut OsRng).unwrap();
    assert_eq!(wrapped_keys.len(), 2);
    assert_eq!(wrapped_keys[0].did, "did:plc:alice");
    assert_eq!(wrapped_keys[1].did, "did:plc:bob");

    let unwrapped_a = unwrap_key(&wrapped_keys[0], &alice.private_keys()).unwrap();
    let unwrapped_b = unwrap_key(&wrapped_keys[1], &bob.private_keys()).unwrap();
    assert_eq!(group_key.0, unwrapped_a.0);
    assert_eq!(group_key.0, unwrapped_b.0);
}

#[test]
fn cross_recipient_splice_rejected() {
    // A wrap targeted at Alice must not be unwrappable by Bob even if Bob has
    // valid hybrid keys — the recipient DID is bound into the HKDF info, and
    // the salt commits to Alice's X25519 pubkey.
    let alice = LocalKeys::generate();
    let bob = LocalKeys::generate();
    let content_key = generate_content_key(&mut OsRng);

    let wrapped_for_alice = wrap_key(
        &content_key,
        &alice.public_keys(),
        "did:plc:alice",
        &mut OsRng,
    )
    .unwrap();
    assert!(unwrap_key(&wrapped_for_alice, &bob.private_keys()).is_err());
}

// -- Wrong-algo rejection --

#[test]
fn unwrap_rejects_unknown_algo() {
    // Build a `WrappedKey` with an unsupported `algo` string — `unwrap_key`
    // must refuse rather than try to parse it as a hybrid envelope.
    use crate::atproto::AtBytes;
    use crate::records::WrappedKey;
    let keys = LocalKeys::generate();
    let bogus = WrappedKey {
        did: "did:plc:test".into(),
        ciphertext: AtBytes {
            encoded: BASE64.encode([0u8; HYBRID_CIPHERTEXT_LEN]),
        },
        algo: "x25519-hkdf-a256kw".into(),
    };
    assert!(unwrap_key(&bogus, &keys.private_keys()).is_err());
}

// -- Ephemeral keypair (hybrid) --

#[test]
fn ephemeral_keypair_has_correct_key_lengths() {
    let kp = generate_ephemeral_keypair(&mut OsRng);
    assert_eq!(kp.x25519_public_key.len(), 32);
    assert_eq!(kp.x25519_private_key.len(), 32);
    assert_eq!(kp.ml_kem_public_key.len(), ML_KEM_PK_LEN);
    assert_eq!(kp.ml_kem_private_key.len(), ML_KEM_SK_LEN);
}

#[test]
fn ephemeral_keypair_unique_each_time() {
    let a = generate_ephemeral_keypair(&mut OsRng);
    let b = generate_ephemeral_keypair(&mut OsRng);
    assert_ne!(a.x25519_public_key, b.x25519_public_key);
    assert_ne!(a.x25519_private_key, b.x25519_private_key);
    assert_ne!(a.ml_kem_public_key, b.ml_kem_public_key);
    assert_ne!(a.ml_kem_private_key, b.ml_kem_private_key);
}

#[test]
fn ephemeral_keypair_roundtrips_through_hybrid_wrap() {
    // The whole reason the ephemeral keypair carries an ML-KEM half is so
    // the pair-flow responder can reach the same hybrid `wrap_key` everyone
    // else uses. Round-trip a content key through that exact path.
    let kp = generate_ephemeral_keypair(&mut OsRng);
    let content_key = generate_content_key(&mut OsRng);

    let wrapped = wrap_key(
        &content_key,
        &kp.public_keys(),
        "did:plc:ephemeral",
        &mut OsRng,
    )
    .unwrap();
    let unwrapped = unwrap_key(&wrapped, &kp.private_keys()).unwrap();
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
