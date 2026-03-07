use anyhow::Result;
use chrono::Utc;
use clap::Args;
use opake_core::client::Session;
use opake_core::crypto::OsRng;
use opake_core::directories;

use crate::commands::{encrypt_directory, Execute};
use crate::identity;
use crate::session::{self, CommandContext};

#[derive(Args)]
/// Create a directory
pub struct MkdirCommand {
    /// Name for the directory
    name: String,
}

impl Execute for MkdirCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        let id = identity::load_identity(&ctx.storage, &ctx.did)?;
        let pubkey = id.public_key_bytes()?;
        let now = Utc::now().to_rfc3339();

        let (root_enc, root_meta) = encrypt_directory("/", &ctx.did, &pubkey, &mut OsRng)?;
        let root_uri =
            directories::get_or_create_root(&mut client, &ctx.did, root_enc, root_meta, &now)
                .await?;

        let (dir_enc, dir_meta) = encrypt_directory(&self.name, &ctx.did, &pubkey, &mut OsRng)?;
        let directory_uri =
            directories::create_directory(&mut client, dir_enc, dir_meta, &now).await?;
        directories::add_entry(&mut client, &root_uri, &directory_uri, &now).await?;

        println!("{} → {}", self.name, directory_uri);

        Ok(session::refreshed_session(&client))
    }
}
