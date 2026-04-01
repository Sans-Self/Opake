use anyhow::Result;
use clap::Args;
use opake_core::client::Session;

use crate::commands::Execute;
use crate::session::CommandContext;

#[derive(Args)]
/// Create a directory
pub struct MkdirCommand {
    /// Name for the directory
    name: String,

    /// Parent directory (path or AT-URI). Defaults to root.
    #[arg(long)]
    dir: Option<String>,

    /// Create under a workspace
    #[arg(long)]
    workspace: Option<String>,
}

impl Execute for MkdirCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut opake = ctx.opake().await?;
        let context = opake.file_context(self.workspace.as_deref()).await?;
        let mut mgr = opake.file_manager(&context);

        let result = mgr
            .create_directory_at(&self.name, self.dir.as_deref())
            .await?;

        let label = self.dir.as_deref().unwrap_or("/");
        if result.outcome.is_proposed() {
            println!("{} → {} (proposed in {})", self.name, result.uri, label);
        } else {
            println!("{} → {} (in {})", self.name, result.uri, label);
        }

        Ok(None)
    }
}
