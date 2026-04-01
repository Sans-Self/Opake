use anyhow::Result;
use clap::Args;
use opake_core::client::Session;
use opake_core::directories::DIRECTORY_COLLECTION;
use opake_core::documents::DOCUMENT_COLLECTION;
use opake_core::keyrings::KEYRING_COLLECTION;
use opake_core::records::{
    PAIR_REQUEST_COLLECTION, PAIR_RESPONSE_COLLECTION, PUBLIC_KEY_COLLECTION,
};
use opake_core::sharing::GRANT_COLLECTION;

use crate::commands::Execute;
use crate::session::CommandContext;

const CONFIRMATION_PHRASE: &str = "I want to delete all my Opake data";

/// All Opake collections in deletion order — dependents before parents.
const COLLECTIONS: &[&str] = &[
    GRANT_COLLECTION,
    PAIR_RESPONSE_COLLECTION,
    PAIR_REQUEST_COLLECTION,
    KEYRING_COLLECTION,
    DOCUMENT_COLLECTION,
    DIRECTORY_COLLECTION,
    PUBLIC_KEY_COLLECTION,
];

/// Delete all Opake data from the PDS
///
/// Permanently deletes all Opake records and blobs from your PDS.
/// This action is irreversible.
#[derive(Args)]
#[command(after_help = "\
Requires typing the exact phrase: \"I want to delete all my Opake data\"

Recommended workflow:
  opake purge --dry-run    # preview what would be deleted
  opake purge              # delete with confirmation prompt
  opake purge --force      # skip all prompts (use with caution)")]
pub struct PurgeCommand {
    /// Show what would be deleted without deleting anything
    #[arg(long)]
    dry_run: bool,

    /// Skip confirmation prompt
    #[arg(long)]
    force: bool,
}

/// Prompt the user to type the exact confirmation phrase.
fn require_confirmation() -> Result<()> {
    println!();
    crate::prompt::confirm_exact(
        "WARNING: This will permanently delete ALL Opake data from your PDS.\n\
         All encrypted files, keys, grants, and directories will be gone.\n\
         This action is irreversible.\n",
        CONFIRMATION_PHRASE,
    )
}

/// Ask whether to delete local identity and session files.
fn confirm_local_cleanup(force: bool) -> Result<bool> {
    if force {
        return Ok(true);
    }

    crate::prompt::confirm("Delete local identity and session?")
}

impl Execute for PurgeCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut opake = ctx.opake().await?;

        if self.dry_run {
            println!("Would delete all records from:");
            for &collection in COLLECTIONS {
                println!("  {collection}");
            }
            println!("\nRun without --dry-run to proceed.");
            return Ok(None);
        }

        // Confirmation gate.
        if !self.force {
            require_confirmation()?;
        }

        // Delete everything.
        let mut total = 0usize;
        for &collection in COLLECTIONS {
            let count = opake.purge_collection(collection).await?;
            if count > 0 {
                println!("deleted {count} {collection}");
            }
            total += count;
        }

        println!();
        if total == 0 {
            println!("No Opake records found on PDS.");
        } else {
            println!("Purged {total} records from PDS.");
        }

        // Local cleanup.
        if confirm_local_cleanup(self.force)? {
            opake.remove_account().await?;
            println!("Removed local identity and session for {}.", ctx.did);
            return Ok(None);
        }

        Ok(None)
    }
}
