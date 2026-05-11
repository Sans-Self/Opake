// opake-crypto: client-side cryptographic primitives.
//
// Lets the `RedactedDebug` derive macro's `::opake_crypto::Redacted` path
// resolve inside this crate.
extern crate self as opake_crypto;

// NOTE TO EDITORS:
// Opake uses a dual-documentation system. If you modify the cryptographic
// primitives, key wrapping schemes, or security model in this crate, you
// MUST also update the corresponding MDX content in `apps/web/src/content/`
// to prevent documentation drift.
//
// AES-256-GCM content encryption and asymmetric key wrapping. Wrapping uses
// the hybrid X25519 + ML-KEM-768 KEM (`x25519-mlkem768-hkdf-a256kw-v2`) —
// defends against harvest-now-decrypt-later per BSI TR-02102 (Germany) and
// ANSSI (France) guidance for hybrid post-quantum key establishment. The pair
// flow generates an ephemeral hybrid bundle on the new device so the same
// construction applies.
//
// The crate has no I/O — it takes bytes in and returns bytes out. The
// calling layer handles reading/writing files and talking to the PDS.
// Randomness is injected via CryptoRng + RngCore parameters so the crate
// stays platform-agnostic — native callers pass OsRng, WASM callers pass a
// crypto.getRandomValues()-backed RNG.

mod at_bytes;
mod content;
pub mod error;
mod key_wrapping;
mod keyring_wrapping;
mod metadata;
mod mnemonic;
mod secrets;
mod wire;

/// Re-export so callers don't need direct rand_core / x25519_dalek / ed25519_dalek dependencies.
pub use aes_gcm::aead::rand_core::{CryptoRng, OsRng, RngCore};
pub use ed25519_dalek::{
    Signature as Ed25519Signature, SigningKey as Ed25519SigningKey,
    VerifyingKey as Ed25519VerifyingKey,
};
pub use x25519_dalek::{
    PublicKey as X25519DalekPublicKey, StaticSecret as X25519DalekStaticSecret,
};

pub use at_bytes::AtBytes;
pub use content::{decrypt_blob, encrypt_blob, generate_content_key};
pub use error::Error;
pub use key_wrapping::{create_group_key, unwrap_key, wrap_key};
// `WrapContext` is part of the public wrap/unwrap surface — callers must
// pass one to scope their wrap to a record context.
pub use keyring_wrapping::{unwrap_content_key_from_keyring, wrap_content_key_for_keyring};
pub use metadata::{
    decrypt_metadata, encrypt_metadata, DirectoryMetadata, DocumentMetadata, GrantMetadata,
    KeyringMetadata,
};
pub use mnemonic::{
    derive_keys_from_mnemonic, format_mnemonic_grid, generate_mnemonic, parse_mnemonic,
    parse_mnemonic_grid, Mnemonic,
};
pub use secrets::DerivedSecrets;
pub use wire::{EncryptedMetadata, WrappedKey};

/// The current app.opake.* schema version this crate implements. Records
/// with version <= this are compatible; higher versions must be rejected by
/// the caller. Also folded into the HKDF info string for domain separation
/// so wraps from one schema version cannot be replayed under a later one.
pub const SCHEMA_VERSION: u32 = 1;

const CONTENT_KEY_LEN: usize = 32;
pub const AES_GCM_NONCE_LEN: usize = 12;
const X25519_KEY_LEN: usize = 32;
const AES_KW_OVERHEAD: usize = 8;
const WRAPPED_KEY_LEN: usize = CONTENT_KEY_LEN + AES_KW_OVERHEAD;

// ───── Hybrid X25519 + ML-KEM-768 KEM ──────────────────────────────────────
//
// Construction aligned with BSI TR-02102 (Germany) and ANSSI guidance for
// hybrid post-quantum key establishment. Algorithm sizes from NIST FIPS-203.

/// Algorithm identifier for the default hybrid wrap envelope, written into
/// `WrappedKey.algo`. The `-v2` suffix tracks the canonical hybrid recipe:
///
/// - HKDF salt commits to the recipient's ML-KEM pubkey alongside the
///   X25519 transcript pieces (X-Wing / BSI-ANSSI worked-example shape).
/// - HKDF info commits to a context tag + scoping URI so a `WrappedKey`
///   lifted from one record context (keyring, document, pair-response,
///   cabinet) cannot be replayed into another.
///
/// Per CLAUDE.md "no install base", v1 envelopes are unreadable under
/// v2 code by design — no migration shim.
pub const HYBRID_WRAP_ALGO: &str = "x25519-mlkem768-hkdf-a256kw-v2";

/// ML-KEM-768 public key size (bytes). NIST FIPS-203 §6.1.
pub const ML_KEM_PK_LEN: usize = 1184;

/// ML-KEM-768 private key size (bytes). NIST FIPS-203 §6.2.
pub const ML_KEM_SK_LEN: usize = 2400;

/// ML-KEM-768 ciphertext size (bytes). NIST FIPS-203 §6.2.
pub(crate) const ML_KEM_CT_LEN: usize = 1088;

/// ML-KEM-768 shared-secret size (bytes). NIST FIPS-203 §6.2.
pub(crate) const ML_KEM_SS_LEN: usize = 32;

/// ML-KEM-768 KeyGen randomness: 32-byte seed `d` ‖ 32-byte implicit-rejection
/// seed `z`. NIST FIPS-203 §7.1.
pub const ML_KEM_KEYGEN_RANDOMNESS_LEN: usize = 64;

/// ML-KEM-768 Encaps randomness: 32-byte message `m`. NIST FIPS-203 §7.2.
pub(crate) const ML_KEM_ENCAP_RANDOMNESS_LEN: usize = 32;

/// Hybrid wrap envelope on the wire:
/// `[X25519 ephemeral pubkey (32) || ML-KEM-768 ciphertext (1088) || AES-KW wrapped content key (40)]`.
pub(crate) const HYBRID_CIPHERTEXT_LEN: usize = X25519_KEY_LEN + ML_KEM_CT_LEN + WRAPPED_KEY_LEN;

// ───────────────────────────────────────────────────────────────────────────

/// Wrapper that prints byte length instead of content in Debug output.
/// Used by the `RedactedDebug` derive macro for `#[redact]` fields.
pub struct Redacted<'a, T: ?Sized>(pub &'a T);

impl std::fmt::Debug for Redacted<'_, String> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{} bytes]", self.0.len())
    }
}

impl std::fmt::Debug for Redacted<'_, Option<String>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(s) => write!(f, "Some([{} bytes])", s.len()),
            None => write!(f, "None"),
        }
    }
}

impl std::fmt::Debug for Redacted<'_, Vec<u8>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{} bytes]", self.0.len())
    }
}

impl<const N: usize> std::fmt::Debug for Redacted<'_, [u8; N]> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{N} bytes]")
    }
}

impl<const N: usize> std::fmt::Debug for Redacted<'_, Option<[u8; N]>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(_) => write!(f, "Some([{N} bytes])"),
            None => write!(f, "None"),
        }
    }
}

/// A 256-bit AES content encryption key.
///
/// Zeroized on drop — RedactedDebug auto-generates Zeroize + Drop for
/// `#[redact]` fields.
#[derive(Clone, opake_derive::RedactedDebug)]
pub struct ContentKey(#[redact] pub [u8; CONTENT_KEY_LEN]);

/// An X25519 public key: 32 raw bytes.
pub type X25519PublicKey = [u8; X25519_KEY_LEN];

/// An X25519 private key: 32 raw bytes.
pub type X25519PrivateKey = [u8; X25519_KEY_LEN];

/// An ML-KEM-768 public key: 1184 raw bytes.
pub type MlKemPublicKey = [u8; ML_KEM_PK_LEN];

/// An ML-KEM-768 private key: 2400 raw bytes. Held as a raw byte array so the
/// `Identity` struct can wrap it in `Zeroizing` / `RedactedDebug` the same way
/// it does for X25519 secrets.
pub type MlKemPrivateKey = [u8; ML_KEM_SK_LEN];

/// A borrowed view of a recipient's hybrid public-key material.
///
/// Used by `wrap_key` and `create_group_key` so the per-call argument list
/// does not grow with every additional KEM half.
#[derive(Debug)]
pub struct PublicKeyBundle<'a> {
    pub x25519: &'a X25519PublicKey,
    pub ml_kem: &'a MlKemPublicKey,
}

/// A borrowed view of one's own hybrid private-key material.
///
/// The `'a` lifetime is the lifetime of whichever struct owns the key bytes
/// — `Cabinet`, `Identity`, or the `DecryptionKeys` helper. Holding raw
/// references avoids copying the 2400-byte ML-KEM private key around the
/// stack on every wrap/unwrap call.
///
/// Manual `Debug` impl elides the raw key bytes — both halves print as their
/// length only, matching the `Redacted` convention.
pub struct PrivateKeyBundle<'a> {
    pub x25519: &'a X25519PrivateKey,
    pub ml_kem: &'a MlKemPrivateKey,
}

impl std::fmt::Debug for PrivateKeyBundle<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrivateKeyBundle")
            .field("x25519", &Redacted(self.x25519))
            .field("ml_kem", &Redacted(self.ml_kem))
            .finish()
    }
}

/// Owned hybrid public-key material, decoded from the encoded base64
/// representation kept in `Identity`.
///
/// Hold one of these on the stack to keep the underlying key bytes alive
/// while a `PublicKeyBundle<'_>` view borrows into it.
pub struct OwnedPublicKeys {
    pub x25519: X25519PublicKey,
    pub ml_kem: MlKemPublicKey,
}

impl OwnedPublicKeys {
    pub fn bundle(&self) -> PublicKeyBundle<'_> {
        PublicKeyBundle {
            x25519: &self.x25519,
            ml_kem: &self.ml_kem,
        }
    }
}

/// Owned hybrid private-key material, zeroized on drop.
///
/// Returned by `Identity::owned_private_keys()` so callers do not have to
/// rebind two locals every time they need a `PrivateKeyBundle<'_>` view.
pub struct OwnedPrivateKeys {
    pub x25519: zeroize::Zeroizing<X25519PrivateKey>,
    pub ml_kem: zeroize::Zeroizing<MlKemPrivateKey>,
}

impl OwnedPrivateKeys {
    pub fn bundle(&self) -> PrivateKeyBundle<'_> {
        PrivateKeyBundle {
            x25519: &self.x25519,
            ml_kem: &self.ml_kem,
        }
    }
}

/// A DID string paired with its hybrid encryption public-key bundle.
///
/// Re-uses `PublicKeyBundle` for the key halves so adding a third KEM
/// half later means touching one struct, not two. The `keys` field is
/// the same view `wrap_key` consumes — no per-member adapter call.
pub struct DidMember<'a> {
    pub did: &'a str,
    pub keys: PublicKeyBundle<'a>,
}

/// An ephemeral hybrid keypair for one-time key exchanges (e.g. device pairing).
///
/// Both halves of the KEM are present so the responding device can wrap an
/// Identity to the requester via the same hybrid construction used everywhere
/// else. The private keys live in this struct only long enough to be persisted
/// to `Storage` (via `save_pair_state`); see the persisted-state docs on the
/// `Storage` trait for the on-disk byte layout.
#[derive(zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct EphemeralKeypair {
    #[zeroize(skip)]
    pub x25519_public_key: X25519PublicKey,
    pub x25519_private_key: X25519PrivateKey,
    #[zeroize(skip)]
    pub ml_kem_public_key: MlKemPublicKey,
    pub ml_kem_private_key: MlKemPrivateKey,
}

impl EphemeralKeypair {
    /// Borrow the public-key halves as a `PublicKeyBundle` view so the
    /// hybrid `wrap_key` can be called against this ephemeral identity.
    pub fn public_keys(&self) -> PublicKeyBundle<'_> {
        PublicKeyBundle {
            x25519: &self.x25519_public_key,
            ml_kem: &self.ml_kem_public_key,
        }
    }

    /// Borrow the private-key halves as a `PrivateKeyBundle` view.
    pub fn private_keys(&self) -> PrivateKeyBundle<'_> {
        PrivateKeyBundle {
            x25519: &self.x25519_private_key,
            ml_kem: &self.ml_kem_private_key,
        }
    }
}

/// Generate a fresh ephemeral hybrid keypair (X25519 + ML-KEM-768) for a
/// one-time exchange. The ML-KEM keygen draws 64 bytes of randomness per
/// FIPS-203 §7.1; both halves use the same RNG.
pub fn generate_ephemeral_keypair(rng: &mut (impl CryptoRng + RngCore)) -> EphemeralKeypair {
    let secret = X25519DalekStaticSecret::random_from_rng(&mut *rng);
    let public = X25519DalekPublicKey::from(&secret);

    let mut mlkem_seed = [0u8; ML_KEM_KEYGEN_RANDOMNESS_LEN];
    rng.fill_bytes(&mut mlkem_seed);
    let mlkem_keypair = libcrux_ml_kem::mlkem768::generate_key_pair(mlkem_seed);
    let mlkem_public_bytes: [u8; ML_KEM_PK_LEN] = (*mlkem_keypair.public_key().as_ref())
        .try_into()
        .expect("ML-KEM-768 public key is 1184 bytes per FIPS-203 §6.1");
    let mlkem_private_bytes: [u8; ML_KEM_SK_LEN] = (*mlkem_keypair.private_key().as_ref())
        .try_into()
        .expect("ML-KEM-768 private key is 2400 bytes per FIPS-203 §6.2");
    mlkem_seed.iter_mut().for_each(|b| *b = 0);

    EphemeralKeypair {
        x25519_public_key: *public.as_bytes(),
        x25519_private_key: secret.to_bytes(),
        ml_kem_public_key: mlkem_public_bytes,
        ml_kem_private_key: mlkem_private_bytes,
    }
}

/// The result of encrypting plaintext content.
///
/// Not redacted — ciphertext and nonces are not secret (sent to PDS).
#[derive(Debug)]
pub struct EncryptedPayload {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; AES_GCM_NONCE_LEN],
}

/// Where a `WrappedKey` lives on the wire. Folded into the HKDF `info`
/// so the wrapping-key derivation depends on the record context — a
/// `WrappedKey` lifted from a keyring and re-published in a document
/// grant (or vice versa) produces a different derived key on unwrap and
/// the AES-KW integrity check fails.
///
/// `Keyring { uri }` and `Document { uri }` carry the record's AT-URI so
/// even two keyrings to the same recipient produce different info
/// strings. `PairResponse` and `Cabinet` are inherently single-context
/// for the local user and don't need a URI.
#[derive(Debug, Clone, Copy)]
pub enum WrapContext<'a> {
    /// Keyring member-key wrap. URI is the keyring's AT-URI.
    Keyring { uri: &'a str },
    /// Document content-key wrap (a grant for an individual recipient).
    /// URI is the document's AT-URI.
    Document { uri: &'a str },
    /// Pair-flow Identity-bundle wrap. The new device's ephemeral keypair
    /// is the only recipient and the pair-response record itself is the
    /// scope; no separate URI needed.
    PairResponse,
    /// Cabinet content-key wrap under the user's own published pubkey.
    /// One cabinet per identity, no URI to disambiguate against.
    Cabinet,
}

impl WrapContext<'_> {
    pub(crate) fn tag(&self) -> &'static str {
        match self {
            WrapContext::Keyring { .. } => "keyring",
            WrapContext::Document { .. } => "document",
            WrapContext::PairResponse => "pair-response",
            WrapContext::Cabinet => "cabinet",
        }
    }

    pub(crate) fn uri(&self) -> &str {
        match self {
            WrapContext::Keyring { uri } | WrapContext::Document { uri } => uri,
            // Static sentinel for contexts that don't carry a URI; gives
            // a stable string for the info transcript without conflating
            // with any real AT-URI (which always start with `at://`).
            WrapContext::PairResponse => "self:pair-response",
            WrapContext::Cabinet => "self:cabinet",
        }
    }
}

/// HKDF info string for domain separation — includes schema version,
/// algorithm identifier, context tag, scoping URI, and recipient DID. A
/// `WrappedKey` lifted between record contexts (keyring → document, or
/// keyring A → keyring B) produces a different derived key on unwrap
/// and AES-KW integrity rejects the replay.
fn hkdf_info(algo: &str, context: &WrapContext<'_>, recipient_did: &str) -> Vec<u8> {
    format!(
        "opake-v{SCHEMA_VERSION}-{algo}-{tag}-{uri}-{recipient_did}",
        tag = context.tag(),
        uri = context.uri(),
    )
    .into_bytes()
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod tests;
