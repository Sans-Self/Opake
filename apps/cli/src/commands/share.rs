use anyhow::Result;
use clap::Args;
use opake_core::client::Session;
use opake_core::crypto::{self, GrantMetadata, OsRng};
use opake_core::error::Error;
use opake_core::records::{PendingShare, PENDING_SHARE_COLLECTION};
use opake_core::resolve;

use crate::commands::Execute;
use crate::session::{self, CommandContext};
use opake_core::client::ReqwestTransport;

#[derive(Args)]
/// Share a document with another user
pub struct NewShareCommand {
    /// AT URI or filename of the document
    document: String,

    /// Handle or DID of the recipient
    recipient: String,

    /// Optional note to the recipient
    #[arg(short, long)]
    note: Option<String>,
}

impl Execute for NewShareCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        // Resolve document name via FileManager
        let mut opake = ctx.opake().await?;
        let context = opake.cabinet_context()?;
        let mut mgr = opake.file_manager(&context);

        let tree = mgr.load_tree().await?;
        let resolved = mgr.resolve_entry(&tree, &self.document).await?;
        let uri = resolved.uri;

        // Resolve recipient identity (separate transport, cross-PDS)
        let transport = ReqwestTransport::new();
        let recipient_result =
            resolve::resolve_identity(&transport, &ctx.pds_url, &self.recipient).await;

        match recipient_result {
            Ok(recipient) => {
                let grant_uri = mgr
                    .share(
                        &uri,
                        &recipient.did,
                        &recipient.public_key,
                        "read",
                        self.note.as_deref(),
                    )
                    .await?;

                let display = recipient.handle.as_deref().unwrap_or(&recipient.did);
                println!("shared with {} → {}", display, grant_uri);
            }
            Err(Error::NotFound(_)) => {
                // Recipient hasn't set up Opake — queue pending share.
                // This path uses fetch_content_key from FileManager, then
                // falls back to raw client for the pending share record.
                let content_key = mgr.fetch_content_key(&uri).await?;

                let metadata = GrantMetadata {
                    permissions: Some("read".to_string()),
                    note: self.note.clone(),
                };
                let encrypted_metadata =
                    crypto::encrypt_metadata(&content_key, &metadata, &mut OsRng)?;

                let now = session::chrono_now();
                let pending =
                    PendingShare::new(uri, self.recipient.clone(), encrypted_metadata, now);

                mgr.create_record(PENDING_SHARE_COLLECTION, &pending)
                    .await?;

                println!(
                    "{} hasn't set up Opake yet. Share queued — it will complete \
                     automatically once they log in (expires in 7 days).",
                    self.recipient
                );
            }
            Err(e) => return Err(e.into()),
        }

        Ok(None)
    }
}

/// List pending (queued) shares.
#[derive(Args)]
pub struct PendingSharesCommand;

impl Execute for PendingSharesCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut opake = ctx.opake().await?;
        let entries = opake.list_pending_shares().await?;

        if entries.is_empty() {
            println!("no pending shares");
        } else {
            for entry in &entries {
                let age = age_display(&entry.created_at);
                println!(
                    "  {} → {} ({}, {})",
                    entry.document, entry.recipient, age, entry.uri
                );
            }
            println!("\n{} pending share(s)", entries.len());
        }

        Ok(None)
    }
}

/// Retry all pending shares now (one-shot).
#[derive(Args)]
pub struct RetrySharesCommand;

impl Execute for RetrySharesCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut opake = ctx.opake().await?;
        let transport = ReqwestTransport::new();
        let result = opake.retry_pending_shares(&transport).await?;

        if result.checked == 0 {
            println!("no pending shares to retry");
        } else {
            println!(
                "{} checked: {} completed, {} expired, {} still pending, {} failed",
                result.checked,
                result.completed,
                result.expired,
                result.still_pending,
                result.failed
            );
        }

        Ok(None)
    }
}

/// Cancel a pending share.
#[derive(Args)]
pub struct CancelShareCommand {
    /// AT-URI of the pending share to cancel
    uri: String,
}

impl Execute for CancelShareCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut opake = ctx.opake().await?;
        opake.cancel_pending_share(&self.uri).await?;
        println!("cancelled pending share {}", self.uri);
        Ok(None)
    }
}

fn age_display(created_at: &str) -> String {
    let now = opake_core::client::time::unix_now();
    let created = opake_core::client::time::parse_rfc3339(created_at).unwrap_or(now);
    let age_hours = (now - created) / 3600;
    if age_hours < 1 {
        "just now".to_string()
    } else if age_hours < 24 {
        format!("{}h ago", age_hours)
    } else {
        format!("{}d ago", age_hours / 24)
    }
}
