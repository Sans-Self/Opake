use anyhow::Result;
use clap::Args;
use opake_core::client::Session;
use opake_core::directories::{check_cycle, EntryKind};

use crate::commands::Execute;
use crate::session::CommandContext;

/// Move a document or directory into another directory
///
/// To rename a document, use `opake metadata rename` instead.
#[derive(Args)]
#[command(after_help = "\
Examples:
  opake move report.pdf projects/
  opake move old-notes/ archive/")]
pub struct MoveCommand {
    /// Source path, filename, or AT-URI
    source: String,

    /// Target directory path or AT-URI (must end with / or resolve to a directory)
    destination: String,
}

impl Execute for MoveCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut opake = ctx.opake().await?;
        let context = opake.cabinet_context()?;
        let mut mgr = opake.file_manager(&context);

        let tree = mgr.load_tree().await?;
        let source = mgr.resolve_entry(&tree, &self.source).await?;
        let dest = tree.resolve_directory(&self.destination)?;

        if source.kind == EntryKind::Directory {
            check_cycle(&tree, &source.uri, &dest.uri)?;
        }

        let source_dir = source
            .parent_uri
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("{:?} has no parent directory", self.source))?;

        mgr.move_entry(&source.uri, source_dir, &dest.uri).await?;

        println!("moved {:?} → {}", source.name, self.destination);

        Ok(None)
    }
}
