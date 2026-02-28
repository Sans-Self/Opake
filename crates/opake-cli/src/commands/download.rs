use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::commands::Execute;

#[derive(Args)]
/// Download and decrypt a file
pub struct DownloadCommand {
    /// AT URI of the document record
    uri: String,

    /// Output path (defaults to the original filename)
    #[arg(short, long)]
    output: Option<PathBuf>,
}

impl Execute for DownloadCommand {
    async fn execute(self) -> Result<()> {
        let _client = crate::config::load_client()?;
        anyhow::bail!("download not yet implemented (tracking: chainlink #6)")
    }
}
