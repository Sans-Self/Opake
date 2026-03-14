use clap::{Args, Subcommand};
use opake_core::client::Session;

use super::Execute;
use crate::session::CommandContext;

use super::{inbox, revoke, share, shared};

/// Share documents and manage grants
///
/// Wraps a document's content key to a recipient's public key,
/// enabling cross-PDS encrypted sharing without federation.
#[derive(Args)]
pub struct ShareGroupCommand {
    #[command(subcommand)]
    action: ShareAction,
}

#[derive(Subcommand)]
enum ShareAction {
    /// Share a document with another user
    ///
    /// Resolves the recipient's encryption key from their PDS and wraps
    /// the document's content key for them. Works across PDS instances.
    New(share::NewShareCommand),
    /// List grants you've shared with others
    List(shared::SharedCommand),
    /// List grants shared with you (via appview)
    Inbox(inbox::InboxCommand),
    /// Revoke a share grant
    ///
    /// Deletes the grant record. The recipient can no longer decrypt
    /// the content key. Note: if the recipient already downloaded and
    /// cached the document, revocation cannot undo that access.
    Revoke(revoke::RevokeCommand),
}

impl Execute for ShareGroupCommand {
    async fn execute(self, ctx: &CommandContext) -> anyhow::Result<Option<Session>> {
        match self.action {
            ShareAction::New(cmd) => cmd.execute(ctx).await,
            ShareAction::List(cmd) => cmd.execute(ctx).await,
            ShareAction::Inbox(cmd) => cmd.execute(ctx).await,
            ShareAction::Revoke(cmd) => cmd.execute(ctx).await,
        }
    }
}
