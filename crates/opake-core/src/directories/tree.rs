// In-memory snapshot of the directory hierarchy for path resolution.
//
// Loads only directory records (one paginated API call). Document names
// are resolved on demand via individual getRecord calls against the
// entries of the relevant directory. This avoids fetching the entire
// document collection for every path-based operation.
//
// Designed for reuse across rm, mv, and any future command that needs
// to resolve user-facing paths to AT-URIs.

use std::collections::HashMap;

use log::debug;

use crate::atproto;
use crate::client::{list_collection, Transport, XrpcClient};
use crate::documents::DOCUMENT_COLLECTION;
use crate::error::Error;
use crate::records::{self, Directory, Document};

use super::{DIRECTORY_COLLECTION, ROOT_DIRECTORY_NAME, ROOT_DIRECTORY_RKEY};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Document,
    Directory,
}

#[derive(Debug, Clone)]
pub struct ResolvedPath {
    pub uri: String,
    pub kind: EntryKind,
    /// Name of the resolved entry.
    pub name: String,
    /// Parent directory URI. None if the target isn't tracked in any directory.
    pub parent_uri: Option<String>,
}

#[derive(Debug)]
struct DirectoryInfo {
    name: String,
    entries: Vec<String>,
}

#[derive(Debug)]
pub struct DirectoryTree {
    /// URI → (name, entries) for every directory record.
    directories: HashMap<String, DirectoryInfo>,
    /// The root directory URI, if it exists.
    root_uri: Option<String>,
}

/// Determine entry kind from the collection segment of an AT-URI.
fn entry_kind_from_uri(uri: &str) -> Option<EntryKind> {
    let at_uri = atproto::parse_at_uri(uri).ok()?;
    match at_uri.collection.as_str() {
        c if c == DOCUMENT_COLLECTION => Some(EntryKind::Document),
        c if c == DIRECTORY_COLLECTION => Some(EntryKind::Directory),
        _ => None,
    }
}

/// Fetch a single document record and return its name.
///
/// Returns None for 404s, unparseable records, and future schema versions
/// (same tolerance as list_collection).
async fn fetch_document_name(
    client: &mut XrpcClient<impl Transport>,
    uri: &str,
) -> Result<Option<String>, Error> {
    let at_uri = atproto::parse_at_uri(uri)?;
    let entry = match client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await
    {
        Ok(e) => e,
        Err(Error::NotFound(_)) => return Ok(None),
        Err(e) => return Err(e),
    };

    let doc: Document = match serde_json::from_value(entry.value) {
        Ok(d) => d,
        Err(_) => return Ok(None),
    };

    if records::check_version(doc.opake_version).is_err() {
        return Ok(None);
    }

    Ok(Some(doc.name))
}

impl DirectoryTree {
    /// Load the directory hierarchy from the PDS.
    ///
    /// Makes one paginated API call (all directories). Documents are NOT
    /// loaded — they're fetched on demand during resolution. The root is
    /// detected from the listing by its rkey ("self").
    pub async fn load(client: &mut XrpcClient<impl Transport>) -> Result<Self, Error> {
        let dir_entries: Vec<(String, DirectoryInfo)> =
            list_collection(client, DIRECTORY_COLLECTION, |uri, dir: Directory| {
                (
                    uri.to_owned(),
                    DirectoryInfo {
                        name: dir.name,
                        entries: dir.entries,
                    },
                )
            })
            .await?;

        let directories: HashMap<String, DirectoryInfo> = dir_entries.into_iter().collect();

        // Find root by rkey — it's the singleton at rkey "self".
        let root_uri = directories
            .keys()
            .find(|uri| {
                atproto::parse_at_uri(uri)
                    .map(|u| u.rkey == ROOT_DIRECTORY_RKEY)
                    .unwrap_or(false)
            })
            .cloned();

        debug!(
            "loaded tree: {} directories, root={}",
            directories.len(),
            root_uri.as_deref().unwrap_or("none"),
        );

        Ok(Self {
            directories,
            root_uri,
        })
    }

    /// Resolve a user-provided reference to an AT-URI with metadata.
    ///
    /// Accepts three forms:
    /// - `at://` URI — directories resolved from memory, documents via getRecord
    /// - Path with `/` — walked segment by segment from root
    /// - Bare name — searched in root's direct children (directories in memory,
    ///   documents via getRecord). Without a root, only directories are searched.
    pub async fn resolve(
        &self,
        client: &mut XrpcClient<impl Transport>,
        reference: &str,
    ) -> Result<ResolvedPath, Error> {
        if reference.starts_with("at://") {
            return self.resolve_at_uri(client, reference).await;
        }

        if reference.contains('/') {
            return self.resolve_path(client, reference).await;
        }

        self.resolve_bare_name(client, reference).await
    }

    /// Load the full directory hierarchy including document names.
    ///
    /// Makes two paginated API calls: one for all directories, one for all
    /// documents. Returns a tree that can render without additional API calls.
    pub async fn load_full(
        client: &mut XrpcClient<impl Transport>,
    ) -> Result<(Self, HashMap<String, String>), Error> {
        let tree = Self::load(client).await?;

        let doc_entries: Vec<(String, String)> =
            list_collection(client, DOCUMENT_COLLECTION, |uri, doc: Document| {
                (uri.to_owned(), doc.name)
            })
            .await?;

        let documents: HashMap<String, String> = doc_entries.into_iter().collect();

        debug!("loaded {} documents for full tree", documents.len());

        Ok((tree, documents))
    }

    /// Build a tree-formatted string of the entire hierarchy.
    ///
    /// Requires the document name map from `load_full`. Entries within each
    /// directory are sorted: directories first (alphabetical), then documents
    /// (alphabetical).
    pub fn render(&self, documents: &HashMap<String, String>) -> String {
        let mut output = String::from(ROOT_DIRECTORY_NAME);

        let root_uri = match &self.root_uri {
            Some(uri) => uri,
            None => return output,
        };

        let dir = match self.directories.get(root_uri) {
            Some(d) => d,
            None => return output,
        };

        let sorted = self.sort_entries(&dir.entries, documents);
        self.render_entries(&sorted, documents, &mut output, "");

        output
    }

    fn sort_entries(
        &self,
        entries: &[String],
        documents: &HashMap<String, String>,
    ) -> Vec<(String, EntryKind, String)> {
        let mut dirs: Vec<(String, EntryKind, String)> = Vec::new();
        let mut docs: Vec<(String, EntryKind, String)> = Vec::new();

        for uri in entries {
            match entry_kind_from_uri(uri) {
                Some(EntryKind::Directory) => {
                    let name = self
                        .directories
                        .get(uri.as_str())
                        .map(|d| d.name.clone())
                        .unwrap_or_else(|| "?".into());
                    dirs.push((uri.clone(), EntryKind::Directory, name));
                }
                Some(EntryKind::Document) => {
                    let name = documents
                        .get(uri.as_str())
                        .cloned()
                        .unwrap_or_else(|| "?".into());
                    docs.push((uri.clone(), EntryKind::Document, name));
                }
                None => {}
            }
        }

        dirs.sort_by(|a, b| a.2.to_lowercase().cmp(&b.2.to_lowercase()));
        docs.sort_by(|a, b| a.2.to_lowercase().cmp(&b.2.to_lowercase()));
        dirs.extend(docs);
        dirs
    }

    fn render_entries(
        &self,
        entries: &[(String, EntryKind, String)],
        documents: &HashMap<String, String>,
        output: &mut String,
        prefix: &str,
    ) {
        let count = entries.len();
        for (i, (uri, kind, name)) in entries.iter().enumerate() {
            let is_last = i == count - 1;
            let connector = if is_last { "└── " } else { "├── " };
            let suffix = if *kind == EntryKind::Directory {
                "/"
            } else {
                ""
            };

            output.push('\n');
            output.push_str(prefix);
            output.push_str(connector);
            output.push_str(name);
            output.push_str(suffix);

            if *kind == EntryKind::Directory {
                if let Some(dir) = self.directories.get(uri.as_str()) {
                    let child_prefix = if is_last {
                        format!("{prefix}    ")
                    } else {
                        format!("{prefix}│   ")
                    };
                    let sorted = self.sort_entries(&dir.entries, documents);
                    self.render_entries(&sorted, documents, output, &child_prefix);
                }
            }
        }
    }

    /// Count descendant documents and directories under a directory URI.
    ///
    /// Infers entry kind from the collection segment in each child URI.
    /// No API calls — works entirely from the loaded directory data.
    pub fn count_descendants(&self, uri: &str) -> (usize, usize) {
        let mut documents = 0usize;
        let mut directories = 0usize;
        let mut stack = vec![uri.to_owned()];

        while let Some(current) = stack.pop() {
            if let Some(dir) = self.directories.get(&current) {
                for entry_uri in &dir.entries {
                    match entry_kind_from_uri(entry_uri) {
                        Some(EntryKind::Document) => documents += 1,
                        Some(EntryKind::Directory) => {
                            directories += 1;
                            if self.directories.contains_key(entry_uri.as_str()) {
                                stack.push(entry_uri.clone());
                            }
                        }
                        None => {}
                    }
                }
            }
        }

        (documents, directories)
    }

    /// Collect all descendant URIs in post-order (children before parents)
    /// for correct deletion ordering.
    ///
    /// No API calls — kind is inferred from the URI collection segment.
    pub fn collect_descendants(&self, uri: &str) -> Vec<(String, EntryKind)> {
        let mut result = Vec::new();
        self.collect_descendants_recursive(uri, &mut result);
        result
    }

    fn collect_descendants_recursive(&self, uri: &str, result: &mut Vec<(String, EntryKind)>) {
        if let Some(dir) = self.directories.get(uri) {
            for entry_uri in &dir.entries {
                match entry_kind_from_uri(entry_uri) {
                    Some(EntryKind::Document) => {
                        result.push((entry_uri.clone(), EntryKind::Document));
                    }
                    Some(EntryKind::Directory) => {
                        if self.directories.contains_key(entry_uri.as_str()) {
                            self.collect_descendants_recursive(entry_uri, result);
                        }
                        result.push((entry_uri.clone(), EntryKind::Directory));
                    }
                    None => {}
                }
            }
        }
    }

    pub async fn resolve_at_uri(
        &self,
        client: &mut XrpcClient<impl Transport>,
        uri: &str,
    ) -> Result<ResolvedPath, Error> {
        // Directories are in memory.
        if let Some(info) = self.directories.get(uri) {
            return Ok(ResolvedPath {
                uri: uri.to_owned(),
                kind: EntryKind::Directory,
                name: info.name.clone(),
                parent_uri: self.find_parent(uri),
            });
        }

        // Documents need a getRecord for the name.
        if let Some(name) = fetch_document_name(client, uri).await? {
            return Ok(ResolvedPath {
                uri: uri.to_owned(),
                kind: EntryKind::Document,
                name,
                parent_uri: self.find_parent(uri),
            });
        }

        Err(Error::NotFound(format!("URI not found: {uri}")))
    }

    async fn resolve_path(
        &self,
        client: &mut XrpcClient<impl Transport>,
        path: &str,
    ) -> Result<ResolvedPath, Error> {
        let root_uri = self
            .root_uri
            .as_ref()
            .ok_or_else(|| Error::NotFound("no root directory — run `opake mkdir` first".into()))?;

        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if segments.is_empty() {
            return Err(Error::InvalidRecord("empty path".into()));
        }

        let mut current_uri = root_uri.clone();

        // Walk all segments except the last one — these must be directories.
        for &segment in &segments[..segments.len() - 1] {
            current_uri = self.find_child_directory(&current_uri, segment)?;
        }

        // Last segment can be either a document or a directory.
        let last = segments[segments.len() - 1];
        self.find_child_any(client, &current_uri, last).await
    }

    async fn resolve_bare_name(
        &self,
        client: &mut XrpcClient<impl Transport>,
        name: &str,
    ) -> Result<ResolvedPath, Error> {
        match &self.root_uri {
            Some(root_uri) => self.find_child_any(client, root_uri, name).await,
            None => {
                // No root — search directories only. Documents should be
                // resolved via documents::resolve_uri before reaching the tree.
                let mut matches = Vec::new();

                for (uri, info) in &self.directories {
                    if info.name == name {
                        matches.push(ResolvedPath {
                            uri: uri.clone(),
                            kind: EntryKind::Directory,
                            name: name.to_owned(),
                            parent_uri: self.find_parent(uri),
                        });
                    }
                }

                match matches.len() {
                    0 => Err(Error::NotFound(format!(
                        "no document or directory named {name:?} — use `opake ls` to see your files"
                    ))),
                    1 => Ok(matches.into_iter().next().unwrap()),
                    n => {
                        let uris: Vec<String> = matches.iter().map(|m| m.uri.clone()).collect();
                        Err(Error::AmbiguousName {
                            name: name.to_owned(),
                            count: n,
                            uris,
                        })
                    }
                }
            }
        }
    }

    /// Find a child directory by name within a parent directory (in memory).
    fn find_child_directory(&self, parent_uri: &str, name: &str) -> Result<String, Error> {
        let parent = self
            .directories
            .get(parent_uri)
            .ok_or_else(|| Error::NotFound(format!("directory not found: {parent_uri}")))?;

        for entry_uri in &parent.entries {
            if let Some(info) = self.directories.get(entry_uri.as_str()) {
                if info.name == name {
                    return Ok(entry_uri.clone());
                }
            }
        }

        Err(Error::NotFound(format!(
            "no directory named {name:?} in {}",
            parent.name,
        )))
    }

    /// Find a child by name in a directory.
    ///
    /// Checks directory children in memory first, then fetches document
    /// children individually via getRecord.
    async fn find_child_any(
        &self,
        client: &mut XrpcClient<impl Transport>,
        parent_uri: &str,
        name: &str,
    ) -> Result<ResolvedPath, Error> {
        let parent = self
            .directories
            .get(parent_uri)
            .ok_or_else(|| Error::NotFound(format!("directory not found: {parent_uri}")))?;

        let mut matches = Vec::new();

        // Directory children: resolved from memory.
        for entry_uri in &parent.entries {
            if let Some(info) = self.directories.get(entry_uri.as_str()) {
                if info.name == name {
                    matches.push(ResolvedPath {
                        uri: entry_uri.clone(),
                        kind: EntryKind::Directory,
                        name: name.to_owned(),
                        parent_uri: Some(parent_uri.to_owned()),
                    });
                }
            }
        }

        // Document children: fetched individually.
        for entry_uri in &parent.entries {
            if entry_kind_from_uri(entry_uri) != Some(EntryKind::Document) {
                continue;
            }

            if let Some(doc_name) = fetch_document_name(client, entry_uri).await? {
                if doc_name == name {
                    matches.push(ResolvedPath {
                        uri: entry_uri.clone(),
                        kind: EntryKind::Document,
                        name: name.to_owned(),
                        parent_uri: Some(parent_uri.to_owned()),
                    });
                }
            }
        }

        match matches.len() {
            0 => Err(Error::NotFound(format!(
                "no document or directory named {name:?} in {}",
                parent.name,
            ))),
            1 => Ok(matches.into_iter().next().unwrap()),
            n => {
                let uris: Vec<String> = matches.iter().map(|m| m.uri.clone()).collect();
                Err(Error::AmbiguousName {
                    name: name.to_owned(),
                    count: n,
                    uris,
                })
            }
        }
    }

    /// Scan all directories to find which one contains the given URI as an entry.
    fn find_parent(&self, child_uri: &str) -> Option<String> {
        for (dir_uri, info) in &self.directories {
            if info.entries.iter().any(|e| e == child_uri) {
                return Some(dir_uri.clone());
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
