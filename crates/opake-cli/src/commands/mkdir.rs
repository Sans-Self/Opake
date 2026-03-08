use anyhow::Result;
use chrono::Utc;
use clap::Args;
use opake_core::client::Session;
use opake_core::crypto::OsRng;
use opake_core::directories::{self, DirectoryTree, EntryKind};
use opake_core::error::Error;

use crate::commands::{encrypt_directory, Execute};
use crate::document_resolve;
use crate::identity;
use crate::session::{self, CommandContext};

#[derive(Args)]
/// Create a directory
pub struct MkdirCommand {
    /// Name for the directory
    name: String,

    /// Parent directory (path, name, or AT-URI). Defaults to root.
    #[arg(long)]
    dir: Option<String>,
}

impl Execute for MkdirCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        let id = identity::load_identity(&ctx.storage, &ctx.did)?;
        let pubkey = id.public_key_bytes()?;
        let private_key = id.private_key_bytes()?;
        let now = Utc::now().to_rfc3339();

        let (root_enc, root_meta) = encrypt_directory("/", &ctx.did, &pubkey, &mut OsRng)?;
        let root_uri =
            directories::get_or_create_root(&mut client, &ctx.did, root_enc, root_meta, &now)
                .await?;

        let mut tree = DirectoryTree::load(&mut client).await?;
        tree.decrypt_names(&ctx.did, &private_key);

        let mut resolver = document_resolve::CliDocumentNameResolver::new(
            &mut client,
            &ctx.did,
            &private_key,
            &ctx.storage,
        );

        let (parent_uri, parent_label) = if let Some(dir_path) = &self.dir {
            let resolved = tree.resolve(&mut resolver, dir_path).await?;

            if resolved.kind != EntryKind::Directory {
                anyhow::bail!("{dir_path:?} is not a directory");
            }
            (resolved.uri, dir_path.as_str())
        } else {
            (root_uri, "/")
        };

        // Check for existing child directory with the same name.
        let full_path = if parent_label == "/" {
            self.name.clone()
        } else {
            format!("{}/{}", parent_label, self.name)
        };
        match tree.resolve(&mut resolver, &full_path).await {
            Err(Error::NotFound(_)) => {}
            Ok(_) | Err(Error::AmbiguousName { .. }) => {
                anyhow::bail!(
                    "directory {:?} already exists in {}",
                    self.name,
                    parent_label
                );
            }
            Err(e) => return Err(e.into()),
        }

        let (dir_enc, dir_meta) = encrypt_directory(&self.name, &ctx.did, &pubkey, &mut OsRng)?;
        let directory_uri =
            directories::create_directory(&mut client, dir_enc, dir_meta, &now).await?;
        directories::add_entry(&mut client, &parent_uri, &directory_uri, &now).await?;

        println!("{} → {} (in {})", self.name, directory_uri, parent_label);

        Ok(session::refreshed_session(&client))
    }
}
