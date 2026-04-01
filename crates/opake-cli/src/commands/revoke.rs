use anyhow::Result;
use clap::Args;
use opake_core::client::Session;

use crate::commands::Execute;
use crate::session::CommandContext;

#[derive(Args)]
/// Revoke a share grant
pub struct RevokeCommand {
    /// AT URI of the grant to revoke
    grant: String,

    /// Skip confirmation prompt
    #[arg(short, long)]
    yes: bool,
}

impl Execute for RevokeCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        if !self.yes && !crate::prompt::confirm(&format!("revoke {}?", self.grant))? {
            println!("aborted");
            return Ok(None);
        }

        let mut opake = ctx.opake().await?;
        let context = opake.cabinet_context()?;
        let mut mgr = opake.file_manager(&context);

        mgr.revoke_share(&self.grant).await?;
        println!("revoked {}", self.grant);

        Ok(None)
    }
}
