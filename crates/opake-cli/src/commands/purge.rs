use anyhow::Result;
use clap::Args;
use opake_core::atproto;
use opake_core::client::{Session, Transport, XrpcClient};
use opake_core::directories::DIRECTORY_COLLECTION;
use opake_core::documents::DOCUMENT_COLLECTION;
use opake_core::keyrings::KEYRING_COLLECTION;
use opake_core::records::{
    PAIR_REQUEST_COLLECTION, PAIR_RESPONSE_COLLECTION, PUBLIC_KEY_COLLECTION,
};
use opake_core::sharing::GRANT_COLLECTION;

use crate::commands::Execute;
use crate::session::{self, CommandContext};

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

/// Paginate through a collection and collect all record keys.
///
/// Unlike `list_collection`, this doesn't parse or version-check records —
/// purge wants to delete everything regardless of schema version.
async fn collect_rkeys(
    client: &mut XrpcClient<impl Transport>,
    collection: &str,
) -> Result<Vec<String>> {
    let mut rkeys = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let page = client
            .list_records(collection, Some(100), cursor.as_deref())
            .await?;

        for record in &page.records {
            let at_uri = atproto::parse_at_uri(&record.uri)?;
            rkeys.push(at_uri.rkey);
        }

        match page.cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }

    Ok(rkeys)
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
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;

        // Enumerate all records across all collections.
        let mut inventory: Vec<(&str, Vec<String>)> = Vec::new();
        let mut total = 0usize;

        for &collection in COLLECTIONS {
            let rkeys = collect_rkeys(&mut client, collection).await?;
            total += rkeys.len();
            inventory.push((collection, rkeys));
        }

        // Nothing to do.
        if total == 0 {
            println!("No Opake records found on PDS.");
            return Ok(session::refreshed_session(&client));
        }

        // Print inventory (always, not just dry-run — useful context before confirmation).
        let max_name_len = inventory
            .iter()
            .map(|(name, _)| name.len())
            .max()
            .unwrap_or(0);

        for &(collection, ref rkeys) in &inventory {
            let noun = if rkeys.len() == 1 {
                "record"
            } else {
                "records"
            };
            println!(
                "  {:<width$}  {:>3} {noun}",
                collection,
                rkeys.len(),
                width = max_name_len,
            );
        }

        println!();

        if self.dry_run {
            let noun = if total == 1 { "record" } else { "records" };
            println!("Total: {total} {noun} would be deleted.");
            return Ok(session::refreshed_session(&client));
        }

        // Confirmation gate.
        if !self.force {
            require_confirmation()?;
        }

        // Delete everything.
        for (collection, rkeys) in &inventory {
            for rkey in rkeys {
                client.delete_record(collection, rkey).await?;
            }
            if !rkeys.is_empty() {
                println!("deleted {} {collection}", rkeys.len());
            }
        }

        println!();
        println!("Purged {total} records from PDS.");

        // Local cleanup.
        if confirm_local_cleanup(self.force)? {
            // Session will be invalid after remove_account, so grab any refresh first.
            let refreshed = session::refreshed_session(&client);
            ctx.storage.remove_account(&ctx.did)?;
            println!("Removed local identity and session for {}.", ctx.did);
            // Don't persist session — the account dir is gone.
            return Ok(refreshed.and(None));
        }

        Ok(session::refreshed_session(&client))
    }
}
