use anyhow::Result;
use clap::Args;
use opake_core::client::Session;
use opake_core::sharing;

use crate::commands::Execute;
use crate::session::{self, CommandContext};

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
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;

        if !self.yes && !crate::prompt::confirm(&format!("revoke {}?", self.grant))? {
            println!("aborted");
            return Ok(session::refreshed_session(&client));
        }

        sharing::revoke_grant(&mut client, &self.grant).await?;
        println!("revoked {}", self.grant);

        Ok(session::refreshed_session(&client))
    }
}
