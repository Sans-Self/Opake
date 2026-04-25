use anyhow::Result;
use clap::Args;
use opake_core::client::Session;

use crate::commands::Execute;
use crate::session::CommandContext;

/// Display directory hierarchy as a tree
///
/// Shows all directories and documents in a nested tree structure.
/// Document names are decrypted client-side from encrypted metadata.
#[derive(Args)]
pub struct TreeCommand {
    /// Show a workspace's directory tree instead of the personal cabinet.
    #[arg(long)]
    workspace: Option<String>,
}

impl Execute for TreeCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut opake = ctx.opake().await?;
        let context = opake.file_context(self.workspace.as_deref()).await?;
        let mut mgr = opake.file_manager(&context);

        let tree = mgr.load_tree().await?;
        let documents = mgr.resolve_document_names(&tree).await?;

        println!("{}", tree.render(&documents));

        Ok(None)
    }
}
