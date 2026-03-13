pub mod accounts;
pub mod cat;
pub mod download;
pub mod inbox;
pub mod keyring;
pub mod login;
pub mod logout;
pub mod ls;
pub mod metadata;
pub mod mkdir;
pub mod move_cmd;
pub mod pair;
pub mod purge;
pub mod recover;
pub mod resolve;
pub mod revoke;
pub mod rm;
pub mod set_default;
pub mod share;
pub mod shared;
pub mod tree;
pub mod upload;

use anyhow::Result;
use opake_core::client::Session;
use opake_core::crypto::{self, DirectoryMetadata};
use opake_core::records::{
    AtBytes, DirectEncryption, EncryptedMetadata, Encryption, EncryptionEnvelope,
};

use crate::session::CommandContext;

pub trait Execute {
    fn execute(
        self,
        ctx: &CommandContext,
    ) -> impl std::future::Future<Output = Result<Option<Session>>>;
}

/// Build a direct encryption envelope for a directory.
///
/// Generates a fresh content key, encrypts the metadata with it, and wraps
/// the key to the owner's public key.
pub fn encrypt_directory(
    name: &str,
    owner_did: &str,
    owner_pubkey: &crypto::X25519PublicKey,
    rng: &mut (impl crypto::CryptoRng + crypto::RngCore),
) -> Result<(Encryption, EncryptedMetadata)> {
    let content_key = crypto::generate_content_key(rng);

    let metadata = DirectoryMetadata {
        name: name.into(),
        description: None,
    };
    let encrypted_metadata = crypto::encrypt_metadata(&content_key, &metadata, rng)?;

    let wrapped_key = crypto::wrap_key(&content_key, owner_pubkey, owner_did, rng)?;

    let encryption = Encryption::Direct(DirectEncryption {
        envelope: EncryptionEnvelope {
            algo: "aes-256-gcm".into(),
            nonce: AtBytes::from_raw(&[0u8; 12]),
            keys: vec![wrapped_key],
        },
    });

    Ok((encryption, encrypted_metadata))
}
