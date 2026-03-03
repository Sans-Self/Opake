// Move and rename operations for documents and directories.
//
// Move = re-parent (remove from old directory, add to new directory).
// Rename = update the record's name field via putRecord.
// Both can happen in a single `opake mv` invocation.

use log::debug;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::documents::DOCUMENT_COLLECTION;
use crate::error::Error;
use crate::records::{self, Directory, Document};

use super::entries::{add_entry, remove_entry};
use super::tree::{DirectoryTree, EntryKind, ResolvedPath};
use super::{DIRECTORY_COLLECTION, ROOT_DIRECTORY_RKEY};

#[derive(Debug)]
pub struct MoveResult {
    pub uri: String,
    pub new_name: Option<String>,
}

/// Move and/or rename a document or directory.
///
/// `destination` is the resolved target — either a directory to move into,
/// or a new name for the source. The caller (CLI) handles the ambiguity of
/// destination interpretation before calling this.
pub async fn move_entry(
    client: &mut XrpcClient<impl Transport>,
    _tree: &DirectoryTree,
    source: &ResolvedPath,
    destination: &MoveDestination,
    modified_at: &str,
) -> Result<MoveResult, Error> {
    match destination {
        MoveDestination::IntoDirectory { directory_uri } => {
            move_into_directory(client, source, directory_uri, modified_at).await
        }
        MoveDestination::Rename { new_name } => {
            rename_entry(client, source, new_name, modified_at).await
        }
        MoveDestination::MoveAndRename {
            directory_uri,
            new_name,
        } => {
            // Can't happen in current CLI UX, but the core supports it.
            move_into_directory(client, source, directory_uri, modified_at).await?;
            rename_entry(client, source, new_name, modified_at).await
        }
    }
}

/// What the destination resolves to.
#[derive(Debug)]
pub enum MoveDestination {
    /// Move into an existing directory, keeping the current name.
    IntoDirectory { directory_uri: String },
    /// Rename in place (no directory change).
    Rename { new_name: String },
    /// Move into a directory AND rename.
    MoveAndRename {
        directory_uri: String,
        new_name: String,
    },
}

/// Check that moving a directory into a target doesn't create a cycle.
///
/// A directory can't be moved into itself or any of its descendants.
pub fn check_cycle(
    tree: &DirectoryTree,
    source_uri: &str,
    target_dir_uri: &str,
) -> Result<(), Error> {
    if source_uri == target_dir_uri {
        return Err(Error::InvalidRecord(
            "cannot move a directory into itself".into(),
        ));
    }

    let descendants = tree.collect_descendants(source_uri);
    for (uri, kind) in &descendants {
        if kind == &EntryKind::Directory && uri == target_dir_uri {
            return Err(Error::InvalidRecord(
                "cannot move a directory into one of its descendants".into(),
            ));
        }
    }

    Ok(())
}

async fn move_into_directory(
    client: &mut XrpcClient<impl Transport>,
    source: &ResolvedPath,
    target_dir_uri: &str,
    modified_at: &str,
) -> Result<MoveResult, Error> {
    // Remove from old parent if tracked.
    if let Some(parent_uri) = &source.parent_uri {
        if parent_uri == target_dir_uri {
            return Err(Error::InvalidRecord(format!(
                "{:?} is already in that directory",
                source.name,
            )));
        }
        debug!("removing {} from old parent {}", source.uri, parent_uri);
        remove_entry(client, parent_uri, &source.uri, modified_at).await?;
    }

    debug!("adding {} to new parent {}", source.uri, target_dir_uri);
    add_entry(client, target_dir_uri, &source.uri, modified_at).await?;

    Ok(MoveResult {
        uri: source.uri.clone(),
        new_name: None,
    })
}

async fn rename_entry(
    client: &mut XrpcClient<impl Transport>,
    source: &ResolvedPath,
    new_name: &str,
    modified_at: &str,
) -> Result<MoveResult, Error> {
    let at_uri = atproto::parse_at_uri(&source.uri)?;

    match source.kind {
        EntryKind::Document => {
            let entry = client
                .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
                .await?;
            let mut doc: Document = serde_json::from_value(entry.value)?;
            records::check_version(doc.version)?;

            doc.name = new_name.to_string();
            doc.modified_at = Some(modified_at.to_string());

            client
                .put_record(DOCUMENT_COLLECTION, &at_uri.rkey, &doc)
                .await?;
        }
        EntryKind::Directory => {
            if at_uri.rkey == ROOT_DIRECTORY_RKEY {
                return Err(Error::InvalidRecord(
                    "cannot rename the root directory".into(),
                ));
            }

            let entry = client
                .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
                .await?;
            let mut dir: Directory = serde_json::from_value(entry.value)?;
            records::check_version(dir.version)?;

            dir.name = new_name.to_string();
            dir.modified_at = Some(modified_at.to_string());

            client
                .put_record(DIRECTORY_COLLECTION, &at_uri.rkey, &dir)
                .await?;
        }
    }

    Ok(MoveResult {
        uri: source.uri.clone(),
        new_name: Some(new_name.to_string()),
    })
}

#[cfg(test)]
#[path = "mv_tests.rs"]
mod tests;
