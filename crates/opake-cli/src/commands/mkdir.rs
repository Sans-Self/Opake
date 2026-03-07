use anyhow::Result;
use chrono::Utc;
use clap::Args;
use opake_core::client::Session;
use opake_core::crypto::{self, DirectoryMetadata, OsRng};
use opake_core::directories;
use opake_core::records::{DirectEncryption, Encryption, EncryptionEnvelope};

use crate::commands::Execute;
use crate::identity;
use crate::session::{self, CommandContext};

#[derive(Args)]
/// Create a directory
pub struct MkdirCommand {
    /// Name for the directory
    name: String,
}

/// Build a direct encryption envelope for a directory.
///
/// Generates a fresh content key, encrypts the metadata with it, and wraps
/// the key to the owner's public key. Returns the encryption enum and
/// encrypted metadata for the directory record.
fn encrypt_directory(
    name: &str,
    owner_did: &str,
    owner_pubkey: &crypto::X25519PublicKey,
    rng: &mut (impl crypto::CryptoRng + crypto::RngCore),
) -> Result<(Encryption, opake_core::records::EncryptedMetadata)> {
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
            nonce: opake_core::records::AtBytes::from_raw(&[0u8; 12]),
            keys: vec![wrapped_key],
        },
    });

    Ok((encryption, encrypted_metadata))
}

impl Execute for MkdirCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        let id = identity::load_identity(&ctx.storage, &ctx.did)?;
        let pubkey = id.public_key_bytes()?;
        let now = Utc::now().to_rfc3339();

        let (root_enc, root_meta) = encrypt_directory("/", &ctx.did, &pubkey, &mut OsRng)?;
        let root_uri =
            directories::get_or_create_root(&mut client, &ctx.did, root_enc, root_meta, &now)
                .await?;

        let (dir_enc, dir_meta) = encrypt_directory(&self.name, &ctx.did, &pubkey, &mut OsRng)?;
        let directory_uri =
            directories::create_directory(&mut client, dir_enc, dir_meta, &now).await?;
        directories::add_entry(&mut client, &root_uri, &directory_uri, &now).await?;

        println!("{} → {}", self.name, directory_uri);

        Ok(session::refreshed_session(&client))
    }
}
