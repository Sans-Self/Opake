use anyhow::Result;
use clap::Args;
use opake_core::client::Session;
use opake_core::directories::EntryKind;

use crate::commands::Execute;
use crate::session::CommandContext;

#[derive(Args)]
/// Delete a document or directory
pub struct RmCommand {
    /// Path, filename, or AT-URI
    reference: String,

    /// Recursively delete directory contents
    #[arg(short = 'r', long)]
    recursive: bool,

    /// Skip confirmation prompt
    #[arg(short, long)]
    yes: bool,
}

impl Execute for RmCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut opake = ctx.opake().await?;
        let context = opake.cabinet_context()?;
        let mut mgr = opake.file_manager(&context);

        let tree = mgr.load_tree().await?;
        let resolved = mgr.resolve_entry(&tree, &self.reference).await?;

        if !self.yes {
            let prompt = match resolved.kind {
                EntryKind::Document => format!("delete {}?", resolved.name),
                EntryKind::Directory => {
                    let (docs, dirs) = tree.count_descendants(&resolved.uri);
                    if docs == 0 && dirs == 0 {
                        format!("delete {}/?", resolved.name)
                    } else if self.recursive {
                        format!(
                            "delete {}/? ({} documents, {} subdirectories)",
                            resolved.name, docs, dirs,
                        )
                    } else {
                        format!("delete {}/? ({} entries)", resolved.name, docs + dirs)
                    }
                }
            };

            if !crate::prompt::confirm(&prompt)? {
                println!("aborted");
                return Ok(None);
            }
        }

        let result = mgr
            .delete_recursive(&tree, &resolved, self.recursive)
            .await?;

        match resolved.kind {
            EntryKind::Document => println!("deleted {}", resolved.uri),
            EntryKind::Directory => {
                if result.documents_deleted == 0 && result.directories_deleted == 1 {
                    println!("deleted {}", resolved.uri);
                } else {
                    println!(
                        "deleted {} ({} documents, {} directories)",
                        resolved.uri, result.documents_deleted, result.directories_deleted,
                    );
                }
            }
        }

        Ok(None)
    }
}
