use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use opake_core::client::Session;
use opake_core::crypto::{parse_mnemonic, parse_mnemonic_grid};
use opake_core::records::{PublicKeyRecord, PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY};
use opake_core::storage::{Identity, Storage};

use crate::commands::Execute;
use crate::identity;
use crate::session::{self, CommandContext};

/// Recover encryption identity from a 24-word seed phrase
///
/// Derives the encryption keypair from a BIP-39 mnemonic. Warns if the
/// derived key does not match the key published on your PDS.
#[derive(Args)]
#[command(after_help = "\
Examples:
  opake recover                    # enter phrase interactively
  opake recover -f seed-backup.txt # read from backup file")]
pub struct RecoverCommand {
    /// Read seed phrase from a .txt backup file instead of stdin
    #[arg(long, short)]
    file: Option<PathBuf>,
}

impl Execute for RecoverCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let did = &ctx.did;

        if identity::load_identity(&ctx.storage, did).is_ok() {
            anyhow::bail!(
                "local identity already exists for {did}. \
                 Delete the identity file first if you want to recover from a different seed phrase."
            );
        }

        let mnemonic = match self.file {
            Some(path) => {
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;
                parse_mnemonic_grid(&content).map_err(|e| {
                    anyhow::anyhow!("invalid seed phrase in {}: {e}", path.display())
                })?
            }
            None => {
                println!("Enter your 24-word seed phrase (space-separated):");
                let entered = crate::prompt::input("> ")?;
                parse_mnemonic(&entered).map_err(|e| anyhow::anyhow!("invalid mnemonic: {e}"))?
            }
        };

        let derived = Identity::from_mnemonic(&mnemonic, did);

        // Use the raw authenticated client for the published-key check: we
        // can't build an Opake yet (no identity in storage), and building one
        // after saving the identity would require a redundant round-trip.
        let mut client = session::load_client(&ctx.storage, did)?;
        let mismatch = check_published_key_mismatch(&mut client, did, &derived).await?;

        if mismatch {
            println!();
            let result = crate::prompt::confirm_exact(
                "WARNING: The derived public key does NOT match the key published on your PDS.\n\
                 This means either:\n\
                 \x20 - The seed phrase is for a different account\n\
                 \x20 - The account's identity was generated randomly (not from a seed phrase)\n\n\
                 Saving this identity will NOT let you decrypt existing data.",
                "save anyway",
            );
            if result.is_err() {
                println!("Recovery cancelled.");
                return Ok(None);
            }
        }

        ctx.storage.save_identity(did, &derived).await?;
        println!("Identity recovered and saved.");

        // Now that identity is on disk, the standard Opake path succeeds.
        let mut opake = ctx.opake().await?;
        opake
            .publish_public_key()
            .await
            .context("failed to publish recovered public key")?;
        println!("Published encryption public key.");

        Ok(None)
    }
}

/// Check if the derived identity's public key matches the one published on PDS.
/// Returns `true` if there's a mismatch, `false` if keys match or no key is published.
async fn check_published_key_mismatch(
    client: &mut opake_core::client::XrpcClient<impl opake_core::client::Transport>,
    did: &str,
    derived: &opake_core::storage::Identity,
) -> Result<bool> {
    let published = client
        .get_record(did, PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY)
        .await;

    match published {
        Ok(record) => {
            let published_key: PublicKeyRecord = serde_json::from_value(record.value)
                .context("failed to parse published public key record")?;
            let published_bytes = published_key
                .x25519_public_key
                .decode()
                .map_err(|e| anyhow::anyhow!("invalid published key: {e}"))?;
            let derived_bytes = derived.x25519_public_key_bytes()?;
            Ok(published_bytes != derived_bytes)
        }
        Err(opake_core::error::Error::NotFound(_)) => Ok(false),
        Err(e) => Err(anyhow::anyhow!("failed to check published key: {e}")),
    }
}
