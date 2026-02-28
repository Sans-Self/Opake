use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::commands::Execute;

#[derive(Args)]
/// Upload and encrypt a file
pub struct UploadCommand {
    /// Path to the file to encrypt and upload
    path: PathBuf,

    /// Encrypt under a keyring instead of direct keys
    #[arg(long)]
    keyring: Option<String>,

    /// Comma-separated tags for categorization
    #[arg(long, value_delimiter = ',')]
    tags: Vec<String>,
}

impl Execute for UploadCommand {
    async fn execute(self) -> Result<()> {
        let _client = crate::config::load_client()?;
        anyhow::bail!("upload not yet implemented (tracking: chainlink #5)")
    }
}
