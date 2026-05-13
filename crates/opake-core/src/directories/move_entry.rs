// Move operations for documents and directories.
//
// Move = re-parent (remove from old directory, add to new directory).
// Rename is handled by the metadata command (#190).

use log::trace;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::error::Error;

use super::entries::{add_entry, remove_entry};
use super::tree::{DirectoryTree, EntryKind, ResolvedPath};

#[derive(Debug)]
pub struct MoveResult {
    pub uri: String,
}

/// Move a document or directory into a target directory.
pub async fn move_entry(
    client: &mut XrpcClient<impl Transport>,
    source: &ResolvedPath,
    target_dir_uri: &str,
    modified_at: &str,
) -> Result<MoveResult, Error> {
    if let Some(parent_uri) = &source.parent_uri {
        if parent_uri == target_dir_uri {
            return Err(Error::InvalidRecord(format!(
                "{:?} is already in that directory",
                source.name,
            )));
        }
        trace!("removing {} from old parent {}", source.uri, parent_uri);
        remove_entry(client, parent_uri, &source.uri, modified_at).await?;
    }

    let parsed = atproto::parse_at_uri(&source.uri)?;
    let entry_record = client
        .get_record(&parsed.authority, &parsed.collection, &parsed.rkey)
        .await?;

    trace!("adding {} to new parent {}", source.uri, target_dir_uri);
    add_entry(
        client,
        target_dir_uri,
        &source.uri,
        &entry_record.cid,
        modified_at,
    )
    .await?;

    Ok(MoveResult {
        uri: source.uri.clone(),
    })
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

#[cfg(test)]
#[path = "move_entry_tests.rs"]
mod tests;
