// In-memory snapshot of the directory hierarchy for path resolution.
//
// Loads directory records in one paginated API call. Document names are
// resolved lazily during path resolution via an async callback trait,
// so only documents in the target directory need to be fetched.

use std::collections::HashMap;

use log::debug;

use crate::atproto;
use crate::client::{list_collection, Transport, XrpcClient};
use crate::crypto::{self, DirectoryMetadata, X25519PrivateKey};
use crate::documents::DOCUMENT_COLLECTION;
use crate::error::Error;
use crate::records::{Directory, Document, EncryptedMetadata, Encryption};

use super::{DIRECTORY_COLLECTION, ROOT_DIRECTORY_NAME, ROOT_DIRECTORY_RKEY};

/// Resolves a document AT-URI to its decrypted name on demand.
///
/// Called lazily during tree path resolution — only for document
/// children of directories actually being searched. Implementations
/// should cache results to avoid repeated PDS fetches.
#[allow(async_fn_in_trait)] // no Send bound needed — used via generics, not dyn; WASM-safe
pub trait DocumentNameResolver {
    async fn resolve_name(&mut self, uri: &str) -> Result<Option<String>, Error>;
}

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
    /// Decrypted name. Empty until `decrypt_names()` is called.
    name: String,
    encryption: Encryption,
    encrypted_metadata: EncryptedMetadata,
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

impl DirectoryTree {
    /// Load the directory hierarchy from the PDS.
    ///
    /// Makes one paginated API call (all directories). Documents are NOT
    /// loaded — callers provide document names separately via `resolve()`.
    /// The root is detected from the listing by its rkey ("self").
    pub async fn load(client: &mut XrpcClient<impl Transport>) -> Result<Self, Error> {
        let dir_entries: Vec<(String, DirectoryInfo)> =
            list_collection(client, DIRECTORY_COLLECTION, |uri, dir: Directory| {
                (
                    uri.to_owned(),
                    DirectoryInfo {
                        name: String::new(),
                        encryption: dir.encryption,
                        encrypted_metadata: dir.encrypted_metadata,
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

    /// Decrypt all directory names in-place.
    ///
    /// Unwraps each directory's content key from the encryption envelope,
    /// then decrypts the metadata to recover the real name. Directories
    /// whose keys can't be unwrapped (wrong DID, keyring not available)
    /// get a fallback name of "?".
    pub fn decrypt_names(&mut self, did: &str, private_key: &X25519PrivateKey) {
        for info in self.directories.values_mut() {
            let content_key = match &info.encryption {
                Encryption::Direct(direct) => {
                    let wrapped = direct.envelope.keys.iter().find(|k| k.did == did);
                    match wrapped {
                        Some(w) => crypto::unwrap_key(w, private_key).ok(),
                        None => None,
                    }
                }
                Encryption::Keyring(_) => {
                    // Keyring-encrypted directories require a group key,
                    // which isn't available here. Future: accept optional
                    // group key map (#191).
                    None
                }
            };

            if let Some(key) = content_key {
                if let Ok(metadata) =
                    crypto::decrypt_metadata::<DirectoryMetadata>(&key, &info.encrypted_metadata)
                {
                    info.name = metadata.name;
                    continue;
                }
            }

            info.name = "?".into();
        }

        // The root directory is always named "/".
        if let Some(root_uri) = &self.root_uri {
            if let Some(info) = self.directories.get_mut(root_uri) {
                info.name = ROOT_DIRECTORY_NAME.into();
            }
        }
    }

    /// Resolve a user-provided reference to an AT-URI with metadata.
    ///
    /// Document names are resolved lazily via the `resolver` callback —
    /// only documents in the target directory are fetched and decrypted.
    ///
    /// Accepts three forms:
    /// - `at://` URI — directories resolved from memory, documents via resolver
    /// - Path with `/` — walked segment by segment from root
    /// - Bare name — searched in root's direct children
    pub async fn resolve(
        &self,
        resolver: &mut impl DocumentNameResolver,
        reference: &str,
    ) -> Result<ResolvedPath, Error> {
        if reference.starts_with("at://") {
            // Directories are in memory.
            if let Some(info) = self.directories.get(reference) {
                return Ok(ResolvedPath {
                    uri: reference.to_owned(),
                    kind: EntryKind::Directory,
                    name: info.name.clone(),
                    parent_uri: self.find_parent(reference),
                });
            }

            // Document URIs — use the rkey as the display name. The caller
            // already has the URI; no PDS fetch needed.
            let at_uri = atproto::parse_at_uri(reference)?;
            if at_uri.collection == DOCUMENT_COLLECTION {
                return Ok(ResolvedPath {
                    uri: reference.to_owned(),
                    kind: EntryKind::Document,
                    name: at_uri.rkey.clone(),
                    parent_uri: self.find_parent(reference),
                });
            }

            return Err(Error::NotFound(format!("URI not found: {reference}")));
        }

        // "/" refers to the root directory.
        if reference.chars().all(|c| c == '/') && !reference.is_empty() {
            let root_uri = self.root_uri.as_ref().ok_or_else(|| {
                Error::NotFound("no root directory — run `opake mkdir` first".into())
            })?;
            return Ok(ResolvedPath {
                uri: root_uri.clone(),
                kind: EntryKind::Directory,
                name: ROOT_DIRECTORY_NAME.into(),
                parent_uri: None,
            });
        }

        if reference.contains('/') {
            return self.resolve_path(resolver, reference).await;
        }

        self.resolve_bare_name(resolver, reference).await
    }

    /// Load the full directory hierarchy including document URIs.
    ///
    /// Makes two paginated API calls: one for all directories, one for all
    /// documents. Returns a tree and a set of document URIs. Callers must
    /// build the name map separately by decrypting metadata.
    pub async fn load_full(
        client: &mut XrpcClient<impl Transport>,
    ) -> Result<(Self, Vec<String>), Error> {
        let tree = Self::load(client).await?;

        let doc_uris: Vec<String> =
            list_collection(client, DOCUMENT_COLLECTION, |uri, _doc: Document| {
                uri.to_owned()
            })
            .await?;

        debug!("loaded {} document URIs for full tree", doc_uris.len());

        Ok((tree, doc_uris))
    }

    /// Build a tree-formatted string of the entire hierarchy.
    ///
    /// Requires a URI → decrypted name map. Entries within each directory
    /// are sorted: directories first (alphabetical), then documents
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

    async fn resolve_path(
        &self,
        resolver: &mut impl DocumentNameResolver,
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
        self.find_child_any(resolver, &current_uri, last).await
    }

    async fn resolve_bare_name(
        &self,
        resolver: &mut impl DocumentNameResolver,
        name: &str,
    ) -> Result<ResolvedPath, Error> {
        match &self.root_uri {
            Some(root_uri) => self.find_child_any(resolver, root_uri, name).await,
            None => {
                // No root — search directories only.
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
    /// Checks directory children in memory, then resolves document
    /// children lazily via the resolver callback.
    async fn find_child_any(
        &self,
        resolver: &mut impl DocumentNameResolver,
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

        // Document children: resolved lazily via the callback.
        for entry_uri in &parent.entries {
            if entry_kind_from_uri(entry_uri) != Some(EntryKind::Document) {
                continue;
            }

            if let Some(doc_name) = resolver.resolve_name(entry_uri).await? {
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
