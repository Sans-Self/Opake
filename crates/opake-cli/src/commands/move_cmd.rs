use anyhow::Result;
use chrono::Utc;
use clap::Args;
use opake_core::client::Session;
use opake_core::directories::{check_cycle, move_entry, DirectoryTree, EntryKind};

use crate::commands::Execute;
use crate::session::{self, CommandContext};

#[derive(Args)]
/// Move a document or directory into another directory
pub struct MoveCommand {
    /// Source path, filename, or AT-URI
    source: String,

    /// Target directory path or AT-URI (must end with / or resolve to a directory)
    destination: String,
}

impl Execute for MoveCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        let now = Utc::now().to_rfc3339();

        let tree = DirectoryTree::load(&mut client).await?;
        let source = tree.resolve(&mut client, &self.source).await?;
        let dest = tree.resolve(&mut client, &self.destination).await?;

        if dest.kind != EntryKind::Directory {
            anyhow::bail!("{:?} is not a directory", self.destination);
        }

        if source.kind == EntryKind::Directory {
            check_cycle(&tree, &source.uri, &dest.uri)?;
        }

        move_entry(&mut client, &source, &dest.uri, &now).await?;

        println!("moved {:?} → {}", source.name, self.destination);

        Ok(session::refreshed_session(&client))
    }
}
