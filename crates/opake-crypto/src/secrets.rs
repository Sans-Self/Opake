// Raw key material for a hybrid identity (X25519 + Ed25519 + ML-KEM-768).
//
// Two construction paths produce the same struct: random generation from an
// injected RNG, or deterministic derivation from a validated BIP-39
// mnemonic. Storage-level wrappers (e.g. opake-core's `Identity`) compose
// around this type by base64-encoding the byte fields.

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{
    CryptoRng, Ed25519SigningKey, MlKemPrivateKey, MlKemPublicKey, RngCore, X25519DalekPublicKey,
    X25519DalekStaticSecret, X25519PrivateKey, X25519PublicKey, ML_KEM_KEYGEN_RANDOMNESS_LEN,
    ML_KEM_PK_LEN, ML_KEM_SK_LEN,
};

const ED25519_KEY_LEN: usize = 32;

/// Raw key material for a hybrid identity. Private halves zeroize on drop.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct DerivedSecrets {
    #[zeroize(skip)]
    pub x25519_public: X25519PublicKey,
    pub x25519_private: X25519PrivateKey,
    #[zeroize(skip)]
    pub ml_kem_public: MlKemPublicKey,
    pub ml_kem_private: MlKemPrivateKey,
    #[zeroize(skip)]
    pub ed25519_verifying: [u8; ED25519_KEY_LEN],
    pub ed25519_signing: [u8; ED25519_KEY_LEN],
}

impl DerivedSecrets {
    /// Generate a fresh random hybrid identity from the injected RNG.
    ///
    /// Counterpart to `derive_keys_from_mnemonic` — both produce the same
    /// shape, only the entropy source differs. The ML-KEM-768 KeyGen draws
    /// 64 bytes per FIPS-203 §7.1.
    pub fn generate(rng: &mut (impl CryptoRng + RngCore)) -> Self {
        let x25519_secret = X25519DalekStaticSecret::random_from_rng(&mut *rng);
        let x25519_public = X25519DalekPublicKey::from(&x25519_secret);

        let ed25519_signing = Ed25519SigningKey::generate(rng);
        let ed25519_verifying = ed25519_signing.verifying_key();

        let mut mlkem_seed = [0u8; ML_KEM_KEYGEN_RANDOMNESS_LEN];
        rng.fill_bytes(&mut mlkem_seed);
        let mlkem_keypair = libcrux_ml_kem::mlkem768::generate_key_pair(mlkem_seed);
        let ml_kem_public: [u8; ML_KEM_PK_LEN] = (*mlkem_keypair.public_key().as_ref())
            .try_into()
            .expect("ML-KEM-768 public key is 1184 bytes per FIPS-203 §6.1");
        let ml_kem_private: [u8; ML_KEM_SK_LEN] = (*mlkem_keypair.private_key().as_ref())
            .try_into()
            .expect("ML-KEM-768 private key is 2400 bytes per FIPS-203 §6.2");
        mlkem_seed.iter_mut().for_each(|b| *b = 0);

        DerivedSecrets {
            x25519_public: *x25519_public.as_bytes(),
            x25519_private: x25519_secret.to_bytes(),
            ml_kem_public,
            ml_kem_private,
            ed25519_verifying: ed25519_verifying.to_bytes(),
            ed25519_signing: ed25519_signing.to_bytes(),
        }
    }
}
