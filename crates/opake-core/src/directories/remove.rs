// Recursive and non-recursive removal of documents and directories.
//
// Operates on a pre-loaded DirectoryTree snapshot. Deletion order is
// post-order (children before parents) so the PDS never sees dangling
// references mid-operation.

use log::debug;

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::error::Error;

use super::entries::remove_entry;
use super::tree::{DirectoryTree, EntryKind, ResolvedPath};
use super::{DIRECTORY_COLLECTION, ROOT_DIRECTORY_RKEY};

#[derive(Debug)]
pub struct RemoveResult {
    pub documents_deleted: usize,
    pub directories_deleted: usize,
}

/// Delete a resolved target (document or directory) from the PDS.
///
/// For directories, `recursive` must be true if the directory has children.
/// The root directory cannot be deleted.
pub async fn remove(
    client: &mut XrpcClient<impl Transport>,
    tree: &DirectoryTree,
    target: &ResolvedPath,
    recursive: bool,
    modified_at: &str,
) -> Result<RemoveResult, Error> {
    match target.kind {
        EntryKind::Document => remove_document(client, target, modified_at).await,
        EntryKind::Directory => {
            remove_directory(client, tree, target, recursive, modified_at).await
        }
    }
}

async fn remove_document(
    client: &mut XrpcClient<impl Transport>,
    target: &ResolvedPath,
    modified_at: &str,
) -> Result<RemoveResult, Error> {
    let at_uri = atproto::parse_at_uri(&target.uri)?;

    debug!("deleting document {}", target.uri);
    client
        .delete_record(&at_uri.collection, &at_uri.rkey)
        .await?;

    if let Some(parent_uri) = &target.parent_uri {
        remove_entry(client, parent_uri, &target.uri, modified_at).await?;
    }

    Ok(RemoveResult {
        documents_deleted: 1,
        directories_deleted: 0,
    })
}

async fn remove_directory(
    client: &mut XrpcClient<impl Transport>,
    tree: &DirectoryTree,
    target: &ResolvedPath,
    recursive: bool,
    modified_at: &str,
) -> Result<RemoveResult, Error> {
    let at_uri = atproto::parse_at_uri(&target.uri)?;

    let is_root = at_uri.rkey == ROOT_DIRECTORY_RKEY;

    let (child_docs, child_dirs) = tree.count_descendants(&target.uri);
    let is_empty = child_docs == 0 && child_dirs == 0;

    if is_root && !recursive {
        return Err(Error::InvalidRecord(
            "cannot delete the root directory — use -r to delete all contents".into(),
        ));
    }

    if !is_empty && !recursive {
        return Err(Error::InvalidRecord(format!(
            "directory is not empty ({} documents, {} subdirectories) — use -r to delete recursively",
            child_docs, child_dirs,
        )));
    }

    let mut documents_deleted = 0;
    let mut directories_deleted = 0;

    if recursive && !is_empty {
        let descendants = tree.collect_descendants(&target.uri);

        for (uri, kind) in &descendants {
            let descendant_uri = atproto::parse_at_uri(uri)?;
            debug!("deleting descendant {}", uri);
            client
                .delete_record(&descendant_uri.collection, &descendant_uri.rkey)
                .await?;

            match kind {
                EntryKind::Document => documents_deleted += 1,
                EntryKind::Directory => directories_deleted += 1,
            }
        }
    }

    debug!("deleting directory {}", target.uri);
    client
        .delete_record(DIRECTORY_COLLECTION, &at_uri.rkey)
        .await?;
    directories_deleted += 1;

    if let Some(parent_uri) = &target.parent_uri {
        remove_entry(client, parent_uri, &target.uri, modified_at).await?;
    }

    Ok(RemoveResult {
        documents_deleted,
        directories_deleted,
    })
}

#[cfg(test)]
#[path = "remove_tests.rs"]
mod tests;
