// Deterministic key derivation from a validated BIP-39 mnemonic.
//
// Pipeline:
//   mnemonic (24 words)
//     → PBKDF2-HMAC-SHA512(password=words, salt="mnemonic", rounds=2048)
//     → 512-bit master seed
//     → HKDF-SHA256(info="opake-v1-x25519-identity")  → X25519 private key
//     → HKDF-SHA256(info="opake-v1-ed25519-signing")   → Ed25519 signing key
//
// No RNG parameter — entirely deterministic. That's an intentional signal.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use hkdf::Hkdf;
use sha2::{Sha256, Sha512};
use zeroize::Zeroizing;

use super::Mnemonic;
use crate::crypto::{Ed25519SigningKey, X25519DalekPublicKey, X25519DalekStaticSecret};
use crate::storage::Identity;

const PBKDF2_ROUNDS: u32 = 2048;
// BIP-39 §"From mnemonic to seed": salt is "mnemonic" + optional passphrase.
// We don't use the passphrase extension, so the salt is just "mnemonic".
// Security comes from the 256-bit entropy in the mnemonic, not the salt.
// https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki#from-mnemonic-to-seed
const PBKDF2_SALT: &[u8] = b"mnemonic";

const HKDF_INFO_X25519: &[u8] = b"opake-v1-x25519-identity";
const HKDF_INFO_ED25519: &[u8] = b"opake-v1-ed25519-signing";

const KEY_LEN: usize = 32;
const MASTER_SEED_LEN: usize = 64;

/// Derive an `Identity` (X25519 + Ed25519 keypairs) from a validated mnemonic.
///
/// Deterministic: the same mnemonic always produces the same keys, regardless
/// of platform. The `did` is stored in the identity but does NOT influence
/// key derivation — the same phrase on a different account yields the same
/// cryptographic material.
pub fn derive_identity_from_mnemonic(mnemonic: &Mnemonic, did: &str) -> Identity {
    let master_seed = derive_master_seed(mnemonic);
    let x25519_raw = derive_key_material(&master_seed, HKDF_INFO_X25519);
    let ed25519_raw = derive_key_material(&master_seed, HKDF_INFO_ED25519);

    let x25519_secret = X25519DalekStaticSecret::from(*x25519_raw);
    let x25519_public = X25519DalekPublicKey::from(&x25519_secret);

    let ed25519_signing = Ed25519SigningKey::from_bytes(&ed25519_raw);
    let ed25519_verifying = ed25519_signing.verifying_key();

    Identity {
        did: did.to_string(),
        public_key: BASE64.encode(x25519_public.as_bytes()),
        private_key: BASE64.encode(x25519_secret.to_bytes()),
        signing_key: Some(BASE64.encode(ed25519_signing.to_bytes())),
        verify_key: Some(BASE64.encode(ed25519_verifying.to_bytes())),
    }
}

/// BIP-39 standard: PBKDF2-HMAC-SHA512, 2048 rounds, salt = "mnemonic".
///
/// The password is the space-joined mnemonic phrase (UTF-8). BIP-39 specifies
/// NFKD normalization, but the English wordlist is pure ASCII so it's a no-op.
fn derive_master_seed(mnemonic: &Mnemonic) -> Zeroizing<[u8; MASTER_SEED_LEN]> {
    let phrase = mnemonic.to_string();
    let mut seed = Zeroizing::new([0u8; MASTER_SEED_LEN]);
    pbkdf2::pbkdf2_hmac::<Sha512>(phrase.as_bytes(), PBKDF2_SALT, PBKDF2_ROUNDS, seed.as_mut());
    seed
}

/// HKDF-SHA256 expansion from the master seed with a domain-separated info string.
fn derive_key_material(
    master_seed: &[u8; MASTER_SEED_LEN],
    info: &[u8],
) -> Zeroizing<[u8; KEY_LEN]> {
    let hkdf = Hkdf::<Sha256>::new(None, master_seed);
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    hkdf.expand(info, key.as_mut())
        .expect("32 bytes is always valid for HKDF-SHA256 output");
    key
}
