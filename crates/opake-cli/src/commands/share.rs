use anyhow::{Context, Result};
use chrono::Utc;
use clap::Args;
use opake_core::client::Session;
use opake_core::crypto::{self, GrantMetadata, OsRng};
use opake_core::directories::DirectoryTree;
use opake_core::documents;
use opake_core::error::Error;
use opake_core::records::{PendingShare, PENDING_SHARE_COLLECTION};
use opake_core::resolve;
use opake_core::sharing::{self, GrantParams};

use crate::commands::Execute;
use crate::document_resolve;
use crate::identity;
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
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        let id =
            identity::load_identity(&ctx.storage, &ctx.did).context("run `opake login` first")?;
        let private_key = id.private_key_bytes()?;

        let mut tree = DirectoryTree::load(&mut client).await?;
        tree.decrypt_names(&ctx.did, &private_key);
        let mut resolver = document_resolve::CliDocumentNameResolver::new(
            &mut client,
            &ctx.did,
            &private_key,
            &ctx.storage,
        );
        let resolved = tree.resolve(&mut resolver, &self.document).await?;
        let uri = resolved.uri;

        let content_key =
            documents::fetch_content_key(&mut client, &id.did, &private_key, &uri).await?;

        let transport = ReqwestTransport::new();
        let now = Utc::now().to_rfc3339();

        match resolve::resolve_identity(&transport, &ctx.pds_url, &self.recipient).await {
            Ok(recipient) => {
                let params = GrantParams {
                    document_uri: &uri,
                    recipient_did: &recipient.did,
                    content_key: &content_key,
                    recipient_public_key: &recipient.public_key,
                    permissions: "read",
                    note: self.note.as_deref(),
                    created_at: &now,
                };

                let grant_uri = sharing::create_grant(&mut client, &params, &mut OsRng).await?;

                let display_recipient = recipient.handle.as_deref().unwrap_or(&recipient.did);
                println!("shared with {} → {}", display_recipient, grant_uri);
            }
            Err(Error::NotFound(_)) => {
                // Recipient hasn't set up Opake yet — queue for retry
                let metadata = GrantMetadata {
                    permissions: Some("read".to_string()),
                    note: self.note.clone(),
                };
                let encrypted_metadata =
                    crypto::encrypt_metadata(&content_key, &metadata, &mut OsRng)?;

                let pending =
                    PendingShare::new(uri, self.recipient.clone(), encrypted_metadata, now);

                client
                    .create_record(PENDING_SHARE_COLLECTION, &pending)
                    .await?;

                println!(
                    "{} hasn't set up Opake yet. Share queued — it will complete \
                     automatically once they log in on any device (expires in 7 days).",
                    self.recipient
                );
            }
            Err(e) => return Err(e.into()),
        }

        Ok(session::refreshed_session(&client))
    }
}

/// List pending (queued) shares.
#[derive(Args)]
pub struct PendingSharesCommand;

impl Execute for PendingSharesCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;

        let entries = sharing::list_pending_shares(&mut client).await?;

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

        Ok(session::refreshed_session(&client))
    }
}

/// Retry all pending shares now (one-shot).
#[derive(Args)]
pub struct RetrySharesCommand;

impl Execute for RetrySharesCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        let id =
            identity::load_identity(&ctx.storage, &ctx.did).context("run `opake login` first")?;
        let private_key = id.private_key_bytes()?;

        let transport = ReqwestTransport::new();
        let now = opake_core::client::time::unix_now();

        let params = sharing::RetryParams {
            caller_pds_url: &ctx.pds_url,
            owner_did: &ctx.did,
            owner_private_key: &private_key,
            now,
            ttl_seconds: sharing::DEFAULT_PENDING_SHARE_TTL_SECONDS,
        };

        let result =
            sharing::retry_pending_shares(&mut client, &transport, &params, &mut OsRng).await?;

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

        Ok(session::refreshed_session(&client))
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
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        sharing::cancel_pending_share(&mut client, &self.uri).await?;
        println!("cancelled pending share {}", self.uri);
        Ok(session::refreshed_session(&client))
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
