use anyhow::Result;
use clap::Args;
use opake_core::client::Session;
use opake_core::error::Error;
use opake_core::resolve::{self, AnchorHistory};

use crate::commands::Execute;
use crate::session::CommandContext;
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

    /// Queue the share if the recipient hasn't set up Opake yet
    #[arg(long)]
    queue: bool,

    /// Authorize one automatic handoff if this not-ready recipient's first
    /// published bundle is unverified. Bound to the DID resolved now.
    #[arg(long)]
    allow_unverified_first_publication: bool,

    /// Confirm the exact currently resolved unverified encryption bundle.
    /// The command prints the recipient DID before using this acknowledgement.
    #[arg(long)]
    approve_unverified: bool,
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
                let approval = mgr.share_approval_challenge(&uri, &recipient);
                if approval.is_some() && !self.approve_unverified {
                    return Err(anyhow::anyhow!(
                        "{} has an unverified encryption key. Review DID {} and re-run with --approve-unverified to share to this exact current bundle.",
                        self.recipient,
                        recipient.did
                    ));
                }
                let write = mgr
                    .share(&uri, &recipient.did, approval, "read", self.note.as_deref())
                    .await?;

                let display = recipient.handle.as_deref().unwrap_or(&recipient.did);
                println!("shared with {} → {}", display, write.uri);
                print_verification_notice(&write.recipient_did, &write.verification);
            }
            Err(Error::RecipientNotReady(_)) => {
                // spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
                println!(
                    "{} exists but hasn't set up Opake yet — they can't receive \
                     this share until they publish an encryption key.",
                    self.recipient
                );

                if self.queue {
                    let pending_recipient = mgr
                        .prepare_pending_share_recipient(&uri, &self.recipient)
                        .await?;
                    if !self.allow_unverified_first_publication {
                        return Err(anyhow::anyhow!(
                            "queuing requires --allow-unverified-first-publication: a background runner cannot ask for consent when {} ({}) first publishes keys",
                            self.recipient,
                            pending_recipient.did(),
                        ));
                    }

                    mgr.create_pending_share(
                        &uri,
                        &pending_recipient,
                        true,
                        "read",
                        self.note.as_deref(),
                    )
                    .await?;
                    println!(
                        "Share queued — it will complete automatically once they \
                         set up Opake (expires in 7 days)."
                    );
                } else {
                    println!("Re-run with --queue to queue the share for when they join.");
                }
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
                let bound_did = match (&entry.recipient_did, &entry.recipient_did_error) {
                    (Some(did), _) => did.clone(),
                    (None, Some(cause)) => format!("unreadable DID: {cause}"),
                    (None, None) => "unreadable DID".to_owned(),
                };
                println!(
                    "  {} → {} [{}] ({}, {})",
                    entry.document, entry.recipient, bound_did, age, entry.uri
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
            for issue in result.verification_errors {
                let expiry = if issue.expired { " (expired)" } else { "" };
                println!(
                    "pending share {} for {}: published key verification failed{}: {}",
                    issue.uri, issue.recipient_did, expiry, issue.reason
                );
            }
            for notice in result.completion_notices {
                print_verification_notice(&notice.did, &notice.verification);
            }
        }

        Ok(None)
    }
}

fn print_verification_notice(did: &str, verification: &resolve::VerificationState) {
    match verification {
        resolve::VerificationState::Unverified => {
            println!("{did}: shared using explicitly approved unverified encryption keys");
        }
        resolve::VerificationState::Verified {
            anchor_history: AnchorHistory::Replaced,
        } => println!("{did}: verification method has changed"),
        resolve::VerificationState::Verified {
            anchor_history: AnchorHistory::NoHistory,
        } => {
            println!("{did}: this DID method publishes no verification history to read");
        }
        resolve::VerificationState::Verified {
            anchor_history: AnchorHistory::Unavailable,
        } => {
            println!(
                "{did}: verification history could not be read; a replacement cannot be ruled out"
            );
        }
        resolve::VerificationState::Verified {
            anchor_history: AnchorHistory::NotReplaced,
        } => {}
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
