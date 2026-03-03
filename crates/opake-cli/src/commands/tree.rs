use anyhow::Result;
use clap::Args;
use opake_core::client::Session;
use opake_core::directories::DirectoryTree;

use crate::commands::Execute;
use crate::session::{self, CommandContext};

#[derive(Args)]
/// Display directory hierarchy as a tree
pub struct TreeCommand;

impl Execute for TreeCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.did)?;
        let (tree, documents) = DirectoryTree::load_full(&mut client).await?;

        println!("{}", tree.render(&documents));

        Ok(session::refreshed_session(&client))
    }
}
