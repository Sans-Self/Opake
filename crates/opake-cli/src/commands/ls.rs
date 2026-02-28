use anyhow::Result;
use clap::Args;

use crate::commands::Execute;

#[derive(Args)]
/// List your documents
pub struct LsCommand {
    /// Filter by tag
    #[arg(long)]
    tag: Option<String>,
}

impl Execute for LsCommand {
    async fn execute(self) -> Result<()> {
        let _client = crate::config::load_client()?;
        anyhow::bail!("ls not yet implemented (tracking: chainlink #7)")
    }
}
