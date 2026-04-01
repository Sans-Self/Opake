use anyhow::Result;
use clap::Args;
use opake_core::client::Session;

use super::download::DownloadCommand;
use crate::commands::Execute;
use crate::session::CommandContext;

/// Print a decrypted file to stdout (alias for `download --stdout`)
#[derive(Args)]
pub struct CatCommand {
    /// Path, filename, or AT-URI of the document
    reference: Option<String>,

    /// Grant URI for downloading a shared file
    #[arg(long, value_name = "AT-URI")]
    grant: Option<String>,

    /// Download a workspace document as a member (cross-PDS, first-time)
    #[arg(long, conflicts_with = "grant", value_name = "AT-URI")]
    workspace_member: Option<String>,

    /// Read from a workspace
    #[arg(long)]
    workspace: Option<String>,
}

impl Execute for CatCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        DownloadCommand {
            reference: self.reference,
            output: None,
            stdout: true,
            grant: self.grant,
            workspace_member: self.workspace_member,
            workspace: self.workspace,
        }
        .execute(ctx)
        .await
    }
}
