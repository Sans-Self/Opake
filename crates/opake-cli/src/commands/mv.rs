use anyhow::Result;
use chrono::Utc;
use clap::Args;
use opake_core::client::Session;
use opake_core::directories::{
    self, check_cycle, move_entry, DirectoryTree, EntryKind, MoveDestination,
};
use opake_core::error::Error as CoreError;

use crate::commands::Execute;
use crate::session::{self, CommandContext};

#[derive(Args)]
/// Move or rename a document or directory
pub struct MvCommand {
    /// Source path, filename, or AT-URI
    source: String,

    /// Destination path or new name
    destination: String,
}

impl Execute for MvCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        let now = Utc::now().to_rfc3339();

        let tree = DirectoryTree::load(&mut client).await?;
        let source = tree.resolve(&mut client, &self.source).await?;

        let destination =
            resolve_destination(&tree, &mut client, &self.destination, &source).await?;

        // Cycle guard for directory moves.
        if source.kind == EntryKind::Directory {
            if let MoveDestination::IntoDirectory { ref directory_uri } = destination {
                check_cycle(&tree, &source.uri, directory_uri)?;
            }
        }

        let result = move_entry(&mut client, &tree, &source, &destination, &now).await?;

        match (&result.new_name, &destination) {
            (Some(new_name), _) => println!("renamed {:?} → {:?}", source.name, new_name),
            (None, MoveDestination::IntoDirectory { .. }) => {
                println!("moved {:?} → {}", source.name, self.destination)
            }
            _ => println!("moved {}", result.uri),
        }

        Ok(session::refreshed_session(&client))
    }
}

/// Interpret the destination argument.
///
/// - Trailing `/` → must resolve to an existing directory
/// - Resolves to an existing directory → move into it
/// - Otherwise → rename (new name = last path segment or bare name)
async fn resolve_destination(
    tree: &DirectoryTree,
    client: &mut opake_core::client::XrpcClient<impl opake_core::client::Transport>,
    destination: &str,
    _source: &directories::ResolvedPath,
) -> Result<MoveDestination, CoreError> {
    let explicit_directory = destination.ends_with('/');
    let trimmed = destination.trim_end_matches('/');

    // Try resolving as path/name in the tree.
    match tree.resolve(client, trimmed).await {
        Ok(resolved) if resolved.kind == EntryKind::Directory => {
            Ok(MoveDestination::IntoDirectory {
                directory_uri: resolved.uri,
            })
        }
        Ok(_) if explicit_directory => Err(CoreError::NotFound(format!(
            "{trimmed:?} is not a directory"
        ))),
        Ok(_) => {
            // Resolved to a document — that's a naming conflict.
            Err(CoreError::InvalidRecord(format!(
                "a document named {trimmed:?} already exists"
            )))
        }
        Err(CoreError::NotFound(_)) if explicit_directory => Err(CoreError::NotFound(format!(
            "directory not found: {trimmed:?}"
        ))),
        Err(CoreError::NotFound(_)) => {
            // Doesn't exist — treat as a rename.
            Ok(MoveDestination::Rename {
                new_name: trimmed.to_string(),
            })
        }
        Err(e) => Err(e),
    }
}
