// Cabinet: the user's personal file space.
//
// Cabinet documents use direct (asymmetric) encryption — content keys are
// wrapped to the owner's hybrid X25519 + ML-KEM-768 public-key bundle. The
// owner is always the caller; there is no member/proposal distinction.

use crate::crypto::{
    MlKemPrivateKey, MlKemPublicKey, PrivateKeyBundle, PublicKeyBundle, X25519PrivateKey,
    X25519PublicKey,
};
use crate::directories::root_directory_uri;
use crate::error::Error;
use crate::storage::Identity;

/// The user's personal file space.
///
/// Holds raw private-key bytes for both halves of the hybrid KEM. Zeroized
/// on drop via the `ZeroizeOnDrop` derive so a dropped cabinet does not
/// leave secret material in memory.
#[derive(Clone, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct Cabinet {
    #[zeroize(skip)]
    pub did: String,
    #[zeroize(skip)]
    pub x25519_public_key: X25519PublicKey,
    pub x25519_private_key: X25519PrivateKey,
    #[zeroize(skip)]
    pub ml_kem_public_key: MlKemPublicKey,
    pub ml_kem_private_key: MlKemPrivateKey,
}

impl Cabinet {
    /// Construct from a loaded identity, decoding key bytes for both halves
    /// of the hybrid KEM.
    pub fn from_identity(identity: &Identity) -> Result<Self, Error> {
        Ok(Self {
            did: identity.did.clone(),
            x25519_public_key: identity.x25519_public_key_bytes()?,
            x25519_private_key: *identity.x25519_private_key_bytes()?,
            ml_kem_public_key: identity.ml_kem_public_key_bytes()?,
            ml_kem_private_key: *identity.ml_kem_private_key_bytes()?,
        })
    }

    /// Borrow the cabinet's public-key halves as a bundle.
    pub fn public_keys(&self) -> PublicKeyBundle<'_> {
        PublicKeyBundle {
            x25519: &self.x25519_public_key,
            ml_kem: &self.ml_kem_public_key,
        }
    }

    /// Borrow the cabinet's private-key halves as a bundle.
    pub fn private_keys(&self) -> PrivateKeyBundle<'_> {
        PrivateKeyBundle {
            x25519: &self.x25519_private_key,
            ml_kem: &self.ml_kem_private_key,
        }
    }

    /// AT-URI for this user's root directory (`at://{did}/at.opake.directory/self`).
    pub fn root_directory_uri(&self) -> String {
        root_directory_uri(&self.did)
    }
}

#[cfg(test)]
#[path = "cabinet_tests.rs"]
mod tests;
