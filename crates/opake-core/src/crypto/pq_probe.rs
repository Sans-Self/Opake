//! Phase 0 probe — verifies the actual shape of libcrux-ml-kem's API
//! against the assumptions in the PQ migration plan, before any of those
//! assumptions get baked into production code paths.
//!
//! Each test pins down one unknown:
//!   1. `round_trip`              — Encap → Decap recovers the same shared secret
//!   2. `keygen_is_deterministic` — same randomness → same keypair (recovery from seed phrase)
//!   3. `encap_is_deterministic`  — same randomness → same ciphertext (testability)
//!   4. `validate_public_key`     — the function exists and accepts a freshly-generated key
//!   5. `key_and_ct_sizes`        — the FIPS-203 byte sizes our envelope assumes
//!
//! WASM compatibility is a separate axis verified by `cargo check
//! -p opake-core --target wasm32-unknown-unknown`, not by an in-process test.
//!
//! When this file lands, the production-side migration (Phases 1+) can rely
//! on the verified API without speculation.

use libcrux_ml_kem::mlkem768;

/// FIPS-203 §7.1 — `KeyGen` consumes 64 bytes of randomness:
/// 32 bytes for the seed `d`, 32 for the implicit-rejection seed `z`.
const KEYGEN_RANDOMNESS_LEN: usize = 64;

/// FIPS-203 §7.2 — `Encaps` consumes 32 bytes of randomness `m`.
const ENCAP_RANDOMNESS_LEN: usize = 32;

/// FIPS-203 ML-KEM-768 published parameter sizes.
const ML_KEM_PK_LEN: usize = 1184;
const ML_KEM_SK_LEN: usize = 2400;
const ML_KEM_CT_LEN: usize = 1088;
const ML_KEM_SS_LEN: usize = 32;

#[test]
fn round_trip() {
    let kg_randomness = [0x42u8; KEYGEN_RANDOMNESS_LEN];
    let key_pair = mlkem768::generate_key_pair(kg_randomness);

    let encap_randomness = [0x37u8; ENCAP_RANDOMNESS_LEN];
    let (ciphertext, shared_secret_sender) =
        mlkem768::encapsulate(key_pair.public_key(), encap_randomness);

    let shared_secret_recipient = mlkem768::decapsulate(key_pair.private_key(), &ciphertext);

    assert_eq!(
        shared_secret_sender.as_ref(),
        shared_secret_recipient.as_ref(),
        "sender and recipient must derive the same shared secret"
    );
}

#[test]
fn keygen_is_deterministic() {
    let randomness = [0x11u8; KEYGEN_RANDOMNESS_LEN];
    let first = mlkem768::generate_key_pair(randomness);
    let second = mlkem768::generate_key_pair(randomness);

    assert_eq!(
        first.public_key().as_ref(),
        second.public_key().as_ref(),
        "same randomness must produce byte-identical public key"
    );
    assert_eq!(
        first.private_key().as_ref(),
        second.private_key().as_ref(),
        "same randomness must produce byte-identical private key"
    );
}

#[test]
fn encap_is_deterministic() {
    let kg_randomness = [0x22u8; KEYGEN_RANDOMNESS_LEN];
    let key_pair = mlkem768::generate_key_pair(kg_randomness);

    let encap_randomness = [0x33u8; ENCAP_RANDOMNESS_LEN];
    let (ct_first, ss_first) = mlkem768::encapsulate(key_pair.public_key(), encap_randomness);
    let (ct_second, ss_second) = mlkem768::encapsulate(key_pair.public_key(), encap_randomness);

    assert_eq!(
        ct_first.as_ref(),
        ct_second.as_ref(),
        "same encap randomness → identical ciphertext"
    );
    assert_eq!(
        ss_first.as_ref(),
        ss_second.as_ref(),
        "same encap randomness → identical shared secret"
    );
}

#[test]
fn validate_public_key_accepts_freshly_generated_key() {
    let kg_randomness = [0x55u8; KEYGEN_RANDOMNESS_LEN];
    let key_pair = mlkem768::generate_key_pair(kg_randomness);

    assert!(
        mlkem768::validate_public_key(key_pair.public_key()),
        "a freshly generated public key must pass FIPS-203 validation"
    );
}

#[test]
fn key_and_ct_sizes_match_fips203() {
    let kg_randomness = [0x66u8; KEYGEN_RANDOMNESS_LEN];
    let key_pair = mlkem768::generate_key_pair(kg_randomness);

    assert_eq!(
        key_pair.public_key().as_ref().len(),
        ML_KEM_PK_LEN,
        "ML-KEM-768 public key must be 1184 bytes"
    );
    assert_eq!(
        key_pair.private_key().as_ref().len(),
        ML_KEM_SK_LEN,
        "ML-KEM-768 private key must be 2400 bytes"
    );

    let encap_randomness = [0x77u8; ENCAP_RANDOMNESS_LEN];
    let (ct, ss) = mlkem768::encapsulate(key_pair.public_key(), encap_randomness);

    assert_eq!(
        ct.as_ref().len(),
        ML_KEM_CT_LEN,
        "ML-KEM-768 ciphertext must be 1088 bytes"
    );
    assert_eq!(
        ss.as_ref().len(),
        ML_KEM_SS_LEN,
        "ML-KEM-768 shared secret must be 32 bytes"
    );
}
