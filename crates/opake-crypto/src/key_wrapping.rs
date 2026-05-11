use aes_kw::KekAes256;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use libcrux_ml_kem::mlkem768::{
    self, MlKem768Ciphertext, MlKem768PrivateKey, MlKem768PublicKey,
};
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::error::Error;
use crate::{
    hkdf_info, AtBytes, ContentKey, CryptoRng, DidMember, PrivateKeyBundle, PublicKeyBundle,
    RngCore, WrapContext, WrappedKey, CONTENT_KEY_LEN, HYBRID_CIPHERTEXT_LEN, HYBRID_WRAP_ALGO,
    ML_KEM_CT_LEN, ML_KEM_ENCAP_RANDOMNESS_LEN, ML_KEM_PK_LEN, ML_KEM_SS_LEN, WRAPPED_KEY_LEN,
    X25519_KEY_LEN,
};

// ───── Hybrid wrap (default) ───────────────────────────────────────────────
//
// Wire envelope:
//   [X25519 ephemeral pub (32) || ML-KEM-768 ciphertext (1088) || AES-KW wrapped (40)]
//
// HKDF combiner (X-Wing / BSI TR-02102 / ANSSI hybrid pattern):
//   ikm    = X25519_shared (32) || ML-KEM_shared (32)
//   salt   = X25519_eph_pub (32) || X25519_recipient_pub (32) ||
//            ML-KEM_recipient_pub (1184) || ML-KEM_ct (1088)
//   info   = "opake-v{ver}-{algo}-{context_tag}-{context_uri}-{recipient_did}"
//
// The salt's transcript binding prevents splice attacks against the KEM
// itself: a flipped ML-KEM ciphertext cannot redirect the wrap to a
// different content key. The info's context binding extends that to record
// boundaries: a `WrappedKey` lifted from one record context (keyring,
// document, pair-response, cabinet) cannot be re-published into another
// because the derived wrapping key would differ.

/// Build the HKDF salt from the wire-format transcript.
///
/// Both static recipient pubkeys are committed alongside the ephemeral
/// X25519 pubkey and the ML-KEM ciphertext — matches the X-Wing /
/// BSI-ANSSI worked-example shape.
fn hybrid_salt(
    eph_x25519_pub: &[u8; X25519_KEY_LEN],
    recipient_x25519_pub: &[u8; X25519_KEY_LEN],
    recipient_ml_kem_pub: &[u8; ML_KEM_PK_LEN],
    ml_kem_ct: &[u8; ML_KEM_CT_LEN],
) -> Vec<u8> {
    let mut salt =
        Vec::with_capacity(X25519_KEY_LEN + X25519_KEY_LEN + ML_KEM_PK_LEN + ML_KEM_CT_LEN);
    salt.extend_from_slice(eph_x25519_pub);
    salt.extend_from_slice(recipient_x25519_pub);
    salt.extend_from_slice(recipient_ml_kem_pub);
    salt.extend_from_slice(ml_kem_ct);
    salt
}

/// HKDF-extract-then-expand from the combined hybrid IKM and the transcript salt.
///
/// IKM is `Zeroizing` so the concatenated raw shared secrets do not linger in
/// the heap after the wrapping key has been derived.
fn derive_hybrid_wrapping_key(
    x25519_shared: &[u8; X25519_KEY_LEN],
    ml_kem_shared: &[u8; ML_KEM_SS_LEN],
    salt: &[u8],
    context: &WrapContext<'_>,
    recipient_did: &str,
) -> Result<Zeroizing<[u8; 32]>, Error> {
    use hkdf::Hkdf;
    use sha2::Sha256;

    let mut ikm: Zeroizing<[u8; X25519_KEY_LEN + ML_KEM_SS_LEN]> = Zeroizing::new([0u8; 64]);
    ikm[..X25519_KEY_LEN].copy_from_slice(x25519_shared);
    ikm[X25519_KEY_LEN..].copy_from_slice(ml_kem_shared);

    let hkdf = Hkdf::<Sha256>::new(Some(salt), &ikm[..]);
    let mut wrapping_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    hkdf.expand(
        &hkdf_info(HYBRID_WRAP_ALGO, context, recipient_did),
        &mut *wrapping_key,
    )
    .map_err(|_| Error::KeyWrap("HKDF expand failed".into()))?;
    Ok(wrapping_key)
}

/// Wrap a content key to a recipient's hybrid public-key bundle.
///
/// Construction: ephemeral X25519 ECDH on the classical side, ML-KEM-768
/// encapsulation on the post-quantum side, HKDF combiner with transcript
/// binding, AES-KW around the resulting 256-bit wrapping key. Aligned with
/// BSI TR-02102 (Germany) and ANSSI guidance for hybrid key establishment.
///
/// Returns a `WrappedKey` whose `ciphertext` carries the full hybrid envelope,
/// base64-encoded for atproto storage.
pub fn wrap_key(
    content_key: &ContentKey,
    recipient: &PublicKeyBundle<'_>,
    recipient_did: &str,
    context: &WrapContext<'_>,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<WrappedKey, Error> {
    // ── Classical half: X25519 ECDH with a fresh ephemeral keypair ──
    let ephemeral_secret = EphemeralSecret::random_from_rng(&mut *rng);
    let ephemeral_public = PublicKey::from(&ephemeral_secret);
    let recipient_x25519 = PublicKey::from(*recipient.x25519);
    let x25519_shared = ephemeral_secret.diffie_hellman(&recipient_x25519);

    // ── Post-quantum half: ML-KEM-768 encapsulation ──
    let recipient_mlkem_pk: MlKem768PublicKey = MlKem768PublicKey::from(*recipient.ml_kem);
    if !mlkem768::validate_public_key(&recipient_mlkem_pk) {
        return Err(Error::KeyWrap(
            "recipient ML-KEM-768 public key failed validation (FIPS-203 §7.2)".into(),
        ));
    }
    let mut encap_randomness = [0u8; ML_KEM_ENCAP_RANDOMNESS_LEN];
    rng.fill_bytes(&mut encap_randomness);
    let (mlkem_ct, mlkem_shared) = mlkem768::encapsulate(&recipient_mlkem_pk, encap_randomness);
    encap_randomness.iter_mut().for_each(|b| *b = 0);

    let mlkem_ct_bytes: [u8; ML_KEM_CT_LEN] = mlkem_ct.into();
    let mlkem_shared_bytes: [u8; ML_KEM_SS_LEN] = (*mlkem_shared.as_ref())
        .try_into()
        .map_err(|_| Error::KeyWrap("ML-KEM-768 shared secret had wrong size".into()))?;

    // ── HKDF combiner with transcript binding ──
    let salt = hybrid_salt(
        ephemeral_public.as_bytes(),
        recipient.x25519,
        recipient.ml_kem,
        &mlkem_ct_bytes,
    );
    let wrapping_key = derive_hybrid_wrapping_key(
        x25519_shared.as_bytes(),
        &mlkem_shared_bytes,
        &salt,
        context,
        recipient_did,
    )?;

    // ── AES-KW around the content key ──
    let kek = KekAes256::new((&*wrapping_key).into());
    let wrapped = kek
        .wrap_vec(&content_key.0)
        .map_err(|_| Error::KeyWrap("AES key wrap failed".into()))?;

    let mut envelope = Vec::with_capacity(HYBRID_CIPHERTEXT_LEN);
    envelope.extend_from_slice(ephemeral_public.as_bytes());
    envelope.extend_from_slice(&mlkem_ct_bytes);
    envelope.extend_from_slice(&wrapped);

    Ok(WrappedKey {
        did: recipient_did.to_string(),
        ciphertext: AtBytes {
            encoded: BASE64.encode(&envelope),
        },
        algo: HYBRID_WRAP_ALGO.to_string(),
    })
}

/// Unwrap a content key using the recipient's hybrid private-key bundle.
///
/// `context` must match the `WrapContext` the wrap side used — passing
/// the wrong one (or none) produces a different derived wrapping key and
/// AES-KW integrity rejects the unwrap. This is the splice protection
/// across record contexts.
pub fn unwrap_key(
    wrapped: &WrappedKey,
    keys: &PrivateKeyBundle<'_>,
    context: &WrapContext<'_>,
) -> Result<ContentKey, Error> {
    if wrapped.algo != HYBRID_WRAP_ALGO {
        return Err(Error::Decryption(format!(
            "expected algo {HYBRID_WRAP_ALGO}, got {}",
            wrapped.algo
        )));
    }
    unwrap_key_hybrid(wrapped, keys, context)
}

/// FIPS-203 ML-KEM-768 decapsulation key layout:
///   dk = dk_PKE (1152) || ek (1184) || H(ek) (32) || z (32)
/// The recipient's encapsulation key (= public key) lives at this offset
/// inside the dk bytes. Extracting it locally on unwrap avoids carrying
/// the 1184-byte pubkey on every `PrivateKeyBundle` just to feed the
/// salt transcript, while still binding to the same bytes the wrap side
/// committed to.
const ML_KEM_PUB_IN_DK_START: usize = 1152;
const ML_KEM_PUB_IN_DK_END: usize = ML_KEM_PUB_IN_DK_START + ML_KEM_PK_LEN;

fn unwrap_key_hybrid(
    wrapped: &WrappedKey,
    keys: &PrivateKeyBundle<'_>,
    context: &WrapContext<'_>,
) -> Result<ContentKey, Error> {
    let envelope = wrapped
        .ciphertext
        .decode()
        .map_err(|e| Error::Decryption(format!("base64 decode: {e}")))?;

    if envelope.len() != HYBRID_CIPHERTEXT_LEN {
        return Err(Error::Decryption(format!(
            "invalid hybrid envelope length: expected {HYBRID_CIPHERTEXT_LEN}, got {}",
            envelope.len()
        )));
    }

    let (eph_pub_bytes, rest) = envelope.split_at(X25519_KEY_LEN);
    let (mlkem_ct_bytes, wrapped_key_bytes) = rest.split_at(ML_KEM_CT_LEN);

    let mut eph_pub: [u8; X25519_KEY_LEN] = [0u8; X25519_KEY_LEN];
    eph_pub.copy_from_slice(eph_pub_bytes);
    let ephemeral_public = PublicKey::from(eph_pub);

    let mut mlkem_ct_arr = [0u8; ML_KEM_CT_LEN];
    mlkem_ct_arr.copy_from_slice(mlkem_ct_bytes);

    // ── Derive recipient's X25519 public key from its private key, so the
    //    salt transcript matches the wrap side without trusting the envelope
    //    to carry recipient_pub redundantly. ──
    let x25519_secret = StaticSecret::from(*keys.x25519);
    let recipient_x25519_pub = PublicKey::from(&x25519_secret);
    let x25519_shared = x25519_secret.diffie_hellman(&ephemeral_public);

    // ── ML-KEM-768 decapsulation ──
    let mlkem_sk_array: Zeroizing<[u8; crate::ML_KEM_SK_LEN]> = Zeroizing::new(*keys.ml_kem);
    let mlkem_sk: MlKem768PrivateKey = MlKem768PrivateKey::from(*mlkem_sk_array);
    let mlkem_ct: MlKem768Ciphertext = MlKem768Ciphertext::from(mlkem_ct_arr);
    let mlkem_shared = mlkem768::decapsulate(&mlkem_sk, &mlkem_ct);
    let mlkem_shared_bytes: Zeroizing<[u8; ML_KEM_SS_LEN]> = Zeroizing::new(
        (*mlkem_shared.as_ref())
            .try_into()
            .map_err(|_| Error::Decryption("ML-KEM-768 shared secret had wrong size".into()))?,
    );

    // Extract recipient's ML-KEM-768 public key from their dk per the
    // FIPS-203 layout. This matches the bytes the wrap side put into
    // the salt transcript without trusting the envelope to redundantly
    // carry the recipient's pubkey.
    let mut recipient_ml_kem_pub = [0u8; ML_KEM_PK_LEN];
    recipient_ml_kem_pub
        .copy_from_slice(&keys.ml_kem[ML_KEM_PUB_IN_DK_START..ML_KEM_PUB_IN_DK_END]);

    let salt = hybrid_salt(
        &eph_pub,
        recipient_x25519_pub.as_bytes(),
        &recipient_ml_kem_pub,
        &mlkem_ct_arr,
    );
    let wrapping_key = derive_hybrid_wrapping_key(
        x25519_shared.as_bytes(),
        &mlkem_shared_bytes,
        &salt,
        context,
        &wrapped.did,
    )?;

    let kek = KekAes256::new((&*wrapping_key).into());
    let content_key_bytes = kek
        .unwrap_vec(wrapped_key_bytes)
        .map_err(|_| Error::Decryption("AES key unwrap failed".into()))?;

    if content_key_bytes.len() != CONTENT_KEY_LEN {
        return Err(Error::Decryption(format!(
            "unwrapped key wrong length: expected {CONTENT_KEY_LEN}, got {}",
            content_key_bytes.len()
        )));
    }

    let mut key = [0u8; CONTENT_KEY_LEN];
    key.copy_from_slice(&content_key_bytes);
    Ok(ContentKey(key))
}

// ───── Group-key creation ──────────────────────────────────────────────────

/// Generate a random group key for a keyring, then wrap it to each member's
/// hybrid public-key bundle. Every wrap is bound to the keyring's URI via
/// `WrapContext::Keyring` so a `WrappedKey` lifted out cannot be replayed
/// in any other record context.
pub fn create_group_key(
    members: &[DidMember],
    keyring_uri: &str,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<(ContentKey, Vec<WrappedKey>), Error> {
    let group_key = crate::generate_content_key(rng);
    let context = WrapContext::Keyring { uri: keyring_uri };
    let wrapped_keys: Result<Vec<_>, _> = members
        .iter()
        .map(|m| wrap_key(&group_key, &m.keys, m.did, &context, rng))
        .collect();
    Ok((group_key, wrapped_keys?))
}

// Compile-time guards: keep the byte-level layout in sync with the FIPS-203
// parameter sizes the rest of the module depends on.
const _: () = assert!(ML_KEM_PK_LEN == 1184);
const _: () = assert!(WRAPPED_KEY_LEN == 40);
const _: () = assert!(HYBRID_CIPHERTEXT_LEN == 32 + 1088 + 40);
