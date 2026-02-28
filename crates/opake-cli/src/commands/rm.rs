use anyhow::Result;
use clap::Args;

use crate::commands::Execute;

#[derive(Args)]
/// Delete a document
pub struct RmCommand {
    /// AT URI of the document record
    uri: String,
}

impl Execute for RmCommand {
    async fn execute(self) -> Result<()> {
        let _client = crate::config::load_client()?;
        anyhow::bail!("rm not yet implemented (tracking: chainlink #8)")
    }
}
