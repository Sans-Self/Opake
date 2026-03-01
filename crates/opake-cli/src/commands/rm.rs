use anyhow::{Context, Result};
use clap::Args;
use opake_core::documents;

use crate::commands::Execute;
use crate::session;

#[derive(Args)]
/// Delete a document
pub struct RmCommand {
    /// AT URI or filename of the document
    reference: String,

    /// Skip confirmation prompt
    #[arg(short, long)]
    yes: bool,
}

impl Execute for RmCommand {
    async fn execute(self) -> Result<()> {
        let client = session::load_client()?;
        let uri = documents::resolve_uri(&client, &self.reference).await?;

        if !self.yes {
            eprint!("delete {}? [y/N] ", uri);
            let mut answer = String::new();
            std::io::stdin()
                .read_line(&mut answer)
                .context("failed to read confirmation")?;
            if !answer.trim().eq_ignore_ascii_case("y") {
                println!("aborted");
                return Ok(());
            }
        }

        documents::delete_document(&client, &uri).await?;
        println!("deleted {}", uri);

        Ok(())
    }
}
