// Cabinet: the user's personal file space.
//
// Cabinet documents use direct (asymmetric) encryption — content keys are
// wrapped to the owner's X25519 public key. The owner is always the caller;
// there is no member/proposal distinction.

use crate::crypto::{X25519PrivateKey, X25519PublicKey};
use crate::directories::root_directory_uri;
use crate::error::Error;
use crate::storage::Identity;

/// The user's personal file space.
///
/// Zeroized on drop — holds raw private key bytes.
#[derive(Clone, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct Cabinet {
    #[zeroize(skip)]
    pub did: String,
    #[zeroize(skip)]
    pub public_key: X25519PublicKey,
    pub private_key: X25519PrivateKey,
}

impl Cabinet {
    /// Construct from a loaded identity, decoding key bytes.
    pub fn from_identity(identity: &Identity) -> Result<Self, Error> {
        Ok(Self {
            did: identity.did.clone(),
            public_key: identity.public_key_bytes()?,
            private_key: *identity.private_key_bytes()?,
        })
    }

    /// AT-URI for this user's root directory (`at://{did}/app.opake.directory/self`).
    pub fn root_directory_uri(&self) -> String {
        root_directory_uri(&self.did)
    }
}

#[cfg(test)]
#[path = "cabinet_tests.rs"]
mod tests;
