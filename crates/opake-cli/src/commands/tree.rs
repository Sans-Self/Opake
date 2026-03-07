use std::collections::HashMap;

use anyhow::Result;
use clap::Args;
use opake_core::client::Session;
use opake_core::directories::DirectoryTree;
use opake_core::documents;

use crate::commands::Execute;
use crate::document_resolve;
use crate::identity;
use crate::session::{self, CommandContext};

#[derive(Args)]
/// Display directory hierarchy as a tree
pub struct TreeCommand;

impl Execute for TreeCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        let mut tree = DirectoryTree::load(&mut client).await?;

        let entries = documents::list_documents(&mut client).await?;
        let id = identity::load_identity(&ctx.storage, &ctx.did)?;
        let private_key = id.private_key_bytes()?;

        tree.decrypt_names(&ctx.did, &private_key);

        let documents: HashMap<String, String> = entries
            .iter()
            .map(|e| {
                let name =
                    document_resolve::decrypt_entry_name(e, &ctx.did, &private_key, &ctx.storage);
                (e.uri.clone(), name)
            })
            .collect();

        println!("{}", tree.render(&documents));

        Ok(session::refreshed_session(&client))
    }
}
