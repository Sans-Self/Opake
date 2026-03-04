use anyhow::{Context, Result};
use chrono::Utc;
use clap::Args;
use opake_core::client::Session;
use opake_core::directories::{self, DirectoryTree, EntryKind, ResolvedPath};
use opake_core::error::Error as CoreError;
use opake_core::{atproto, documents};

use crate::commands::Execute;
use crate::session::{self, CommandContext};

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

/// Determine whether a reference needs the full directory tree or can use
/// the lightweight document-only resolver.
///
/// Paths with `/` always need the tree. AT-URIs targeting directories need
/// the tree. Bare names try document resolution first, falling back to the
/// tree only when no document matches.
enum Resolution {
    /// Resolved cheaply without building the full tree.
    Fast(ResolvedPath),
    /// Needs the full directory tree (path resolution, directory target, etc).
    NeedsTree,
}

async fn try_fast_resolve(
    client: &mut opake_core::client::XrpcClient<impl opake_core::client::Transport>,
    reference: &str,
) -> Result<Resolution, CoreError> {
    // Paths always need the tree.
    if reference.contains('/') {
        return Ok(Resolution::NeedsTree);
    }

    // AT-URIs targeting directories need the tree for emptiness checks / recursion.
    if reference.starts_with("at://") {
        let at_uri = atproto::parse_at_uri(reference)?;
        if at_uri.collection == directories::DIRECTORY_COLLECTION {
            return Ok(Resolution::NeedsTree);
        }
        // Document AT-URI — no tree needed, no parent cleanup.
        return Ok(Resolution::Fast(ResolvedPath {
            uri: reference.to_owned(),
            kind: EntryKind::Document,
            name: at_uri.rkey.clone(),
            parent_uri: None,
        }));
    }

    // Bare name — try document-only resolution (1 paginated API call).
    match documents::resolve_uri(client, reference).await {
        Ok(uri) => Ok(Resolution::Fast(ResolvedPath {
            uri,
            kind: EntryKind::Document,
            name: reference.to_owned(),
            parent_uri: None,
        })),
        // No document match — might be a directory name. Need the tree.
        Err(CoreError::NotFound(_)) => Ok(Resolution::NeedsTree),
        Err(e) => Err(e),
    }
}

impl Execute for RmCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        let now = Utc::now().to_rfc3339();

        let resolution = try_fast_resolve(&mut client, &self.reference).await?;

        // Fast path: document by bare name or AT-URI, no tree needed.
        if let Resolution::Fast(resolved) = resolution {
            if !self.yes {
                eprint!("delete {}? [y/N] ", resolved.name);
                let mut answer = String::new();
                std::io::stdin()
                    .read_line(&mut answer)
                    .context("failed to read confirmation")?;
                if !answer.trim().eq_ignore_ascii_case("y") {
                    println!("aborted");
                    return Ok(session::refreshed_session(&client));
                }
            }

            documents::delete_document(&mut client, &resolved.uri).await?;
            println!("deleted {}", resolved.uri);
            return Ok(session::refreshed_session(&client));
        }

        // Full tree path: paths, directories, recursive deletion.
        let tree = DirectoryTree::load(&mut client).await?;
        let resolved = tree.resolve(&mut client, &self.reference).await?;

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

            eprint!("{prompt} [y/N] ");
            let mut answer = String::new();
            std::io::stdin()
                .read_line(&mut answer)
                .context("failed to read confirmation")?;
            if !answer.trim().eq_ignore_ascii_case("y") {
                println!("aborted");
                return Ok(session::refreshed_session(&client));
            }
        }

        let result =
            directories::remove(&mut client, &tree, &resolved, self.recursive, &now).await?;

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

        Ok(session::refreshed_session(&client))
    }
}
