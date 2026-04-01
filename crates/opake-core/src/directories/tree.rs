// In-memory snapshot of the directory hierarchy for path resolution.
//
// Loads directory records in one paginated API call. Document names are
// resolved lazily during path resolution via an async callback trait,
// so only documents in the target directory need to be fetched.

use std::collections::HashMap;

use log::trace;

use crate::atproto;
use crate::client::TreeDirectory;
use crate::crypto::{self, DirectoryMetadata, X25519PrivateKey};
use crate::documents::DOCUMENT_COLLECTION;
use crate::error::Error;
use crate::records::{Directory, EncryptedMetadata, KeyWrapping};
use crate::storage::CachedRecord;

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
    key_wrapping: KeyWrapping,
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
    /// Build a tree from pre-fetched directory records.
    ///
    /// Accepts (AT-URI, Directory) pairs — the same shape returned by
    /// `listRecords`. The root is detected by rkey `"self"`.
    pub fn from_records(records: impl IntoIterator<Item = (String, Directory)>) -> Self {
        let directories: HashMap<String, DirectoryInfo> = records
            .into_iter()
            .map(|(uri, dir)| {
                (
                    uri,
                    DirectoryInfo {
                        name: String::new(),
                        key_wrapping: dir.key_wrapping,
                        encrypted_metadata: dir.encrypted_metadata,
                        entries: dir.entries,
                    },
                )
            })
            .collect();

        let root_uri = directories
            .keys()
            .find(|uri| {
                atproto::parse_at_uri(uri)
                    .map(|u| u.rkey == ROOT_DIRECTORY_RKEY)
                    .unwrap_or(false)
            })
            .cloned();

        trace!(
            "built tree: {} directories, root={}",
            directories.len(),
            root_uri.as_deref().unwrap_or("none"),
        );

        Self {
            directories,
            root_uri,
        }
    }

    /// Override the root directory URI.
    ///
    /// Used for workspace trees where the root is not `directory/self` but
    /// a deterministic `directory/ws-{keyring_rkey}`. Returns false if the
    /// URI isn't in the tree (caller should handle the error).
    pub fn set_root(&mut self, uri: &str) -> bool {
        if self.directories.contains_key(uri) {
            self.root_uri = Some(uri.to_owned());
            true
        } else {
            false
        }
    }

    /// Load the directory hierarchy from the PDS (test use only).
    ///
    /// Production code uses AppView snapshots via `from_cached_records()`.
    #[cfg(test)]
    pub(crate) async fn load(
        client: &mut crate::client::XrpcClient<impl crate::client::Transport>,
    ) -> Result<Self, Error> {
        let dir_entries: Vec<(String, Directory)> =
            crate::client::list_collection(client, DIRECTORY_COLLECTION, |uri, dir: Directory| {
                (uri.to_owned(), dir)
            })
            .await?;

        Ok(Self::from_records(dir_entries))
    }

    /// Decrypt all directory names in-place.
    ///
    /// Unwraps each directory's content key from the encryption envelope,
    /// then decrypts the metadata to recover the real name. Directories
    /// whose keys can't be unwrapped (wrong DID, keyring not available)
    /// get a fallback name of "?".
    pub fn decrypt_names(&mut self, did: &str, private_key: &X25519PrivateKey) {
        self.decrypt_names_with_group_keys(did, private_key, &HashMap::new());
    }

    /// Decrypt all directory names in-place, with group key support.
    ///
    /// Like [`decrypt_names`], but also handles keyring-encrypted directories
    /// using the provided group key map (keyring URI → group key).
    pub fn decrypt_names_with_group_keys(
        &mut self,
        did: &str,
        private_key: &X25519PrivateKey,
        group_keys: &HashMap<String, crypto::ContentKey>,
    ) {
        for info in self.directories.values_mut() {
            let content_key = match &info.key_wrapping {
                KeyWrapping::Direct(direct) => {
                    let wrapped = direct.keys.iter().find(|k| k.did == did);
                    match wrapped {
                        Some(w) => crypto::unwrap_key(w, private_key).ok(),
                        None => None,
                    }
                }
                KeyWrapping::Keyring(kr) => {
                    let keyring_uri = &kr.keyring_ref.keyring;
                    group_keys.get(keyring_uri).and_then(|gk| {
                        let wrapped_bytes = kr.keyring_ref.wrapped_content_key.decode().ok()?;
                        crypto::unwrap_content_key_from_keyring(&wrapped_bytes, gk).ok()
                    })
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

    // -----------------------------------------------------------------------
    // Public getters (used by WASM handle + CLI)
    // -----------------------------------------------------------------------

    pub fn root_uri(&self) -> Option<&str> {
        self.root_uri.as_deref()
    }

    /// Returns the child entry URIs for a directory, or None if the URI
    /// is not a known directory.
    pub fn entries_for(&self, uri: &str) -> Option<&[String]> {
        self.directories
            .get(uri)
            .map(|info| info.entries.as_slice())
    }

    /// Returns the decrypted name for a directory URI.
    pub fn directory_name(&self, uri: &str) -> Option<&str> {
        self.directories.get(uri).map(|info| info.name.as_str())
    }

    /// Whether the given URI is a known directory in this tree.
    pub fn is_directory(&self, uri: &str) -> bool {
        self.directories.contains_key(uri)
    }

    /// Whether the given URI looks like a document URI (by collection segment).
    pub fn is_document_uri(&self, uri: &str) -> bool {
        entry_kind_from_uri(uri) == Some(EntryKind::Document)
    }

    /// Iterate over all directory URIs in the tree.
    pub fn all_directory_uris(&self) -> impl Iterator<Item = &str> {
        self.directories.keys().map(String::as_str)
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
    /// Collect all document URIs reachable from the current root.
    ///
    /// Walks the directory subtree from root and returns only document URIs.
    /// Useful for scoping document name resolution to just the visible tree.
    pub fn document_uris_in_subtree(&self) -> Vec<String> {
        let root = match &self.root_uri {
            Some(uri) => uri.as_str(),
            None => return Vec::new(),
        };
        self.collect_descendants(root)
            .into_iter()
            .filter(|(_, kind)| *kind == EntryKind::Document)
            .map(|(uri, _)| uri)
            .collect()
    }

    /// Collect all descendant URIs in post-order (children before parents).
    ///
    /// Uses an explicit stack instead of recursion. Post-order ensures
    /// directories appear after their contents — correct for deletion.
    pub fn collect_descendants(&self, uri: &str) -> Vec<(String, EntryKind)> {
        let mut result = Vec::new();
        let mut stack: Vec<(String, bool)> = vec![(uri.to_owned(), false)];

        while let Some((current, visited)) = stack.pop() {
            let Some(dir) = self.directories.get(&current) else {
                continue;
            };

            if visited {
                // Post-order: emit the directory after its children
                if current != uri {
                    result.push((current, EntryKind::Directory));
                }
                continue;
            }

            // Push self back as visited, then push children
            stack.push((current.clone(), true));

            for entry_uri in dir.entries.iter().rev() {
                match entry_kind_from_uri(entry_uri) {
                    Some(EntryKind::Document) => {
                        result.push((entry_uri.clone(), EntryKind::Document));
                    }
                    Some(EntryKind::Directory) => {
                        if self.directories.contains_key(entry_uri.as_str()) {
                            stack.push((entry_uri.clone(), false));
                        } else {
                            result.push((entry_uri.clone(), EntryKind::Directory));
                        }
                    }
                    None => {}
                }
            }
        }

        result
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

    /// Resolve a path to a directory. Fails if any segment is not a directory.
    ///
    /// Unlike `resolve()`, this never needs a `DocumentNameResolver` — directory
    /// names are already decrypted in the tree. Use this when you know the target
    /// must be a directory (e.g., `--dir` flags, parent resolution).
    pub fn resolve_directory(&self, path: &str) -> Result<ResolvedPath, Error> {
        // "/" or "///" → root
        if path.chars().all(|c| c == '/') && !path.is_empty() {
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

        // AT-URI passthrough
        if path.starts_with("at://") {
            let info = self
                .directories
                .get(path)
                .ok_or_else(|| Error::NotFound(format!("directory not found: {path}")))?;
            return Ok(ResolvedPath {
                uri: path.to_owned(),
                kind: EntryKind::Directory,
                name: info.name.clone(),
                parent_uri: self.find_parent(path),
            });
        }

        let root_uri = self
            .root_uri
            .as_ref()
            .ok_or_else(|| Error::NotFound("no root directory — run `opake mkdir` first".into()))?;

        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if segments.is_empty() {
            return Err(Error::InvalidRecord("empty path".into()));
        }

        let mut current_uri = root_uri.clone();
        let mut parent_uri: Option<String> = None;

        for &segment in &segments {
            parent_uri = Some(current_uri.clone());
            current_uri = self.find_child_directory(&current_uri, segment)?;
        }

        let name = self
            .directories
            .get(&current_uri)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "?".into());

        Ok(ResolvedPath {
            uri: current_uri,
            kind: EntryKind::Directory,
            name,
            parent_uri,
        })
    }

    /// Check whether a directory has a child directory with the given name.
    pub fn has_child_directory(&self, parent_uri: &str, name: &str) -> bool {
        let Some(parent) = self.directories.get(parent_uri) else {
            return false;
        };
        parent.entries.iter().any(|entry_uri| {
            self.directories
                .get(entry_uri.as_str())
                .is_some_and(|info| info.name == name)
        })
    }

    /// Scan all directories to find which one contains the given URI as an entry.
    pub fn find_parent(&self, child_uri: &str) -> Option<String> {
        for (dir_uri, info) in &self.directories {
            if info.entries.iter().any(|e| e == child_uri) {
                return Some(dir_uri.clone());
            }
        }
        None
    }

    // -----------------------------------------------------------------------
    // Cache integration
    // -----------------------------------------------------------------------

    /// Build a tree from cached records stored via `Storage::cache_put_collection`.
    ///
    /// Each `CachedRecord.value` is deserialized as a `Directory`. Records
    /// that fail deserialization are silently skipped (stale cache entries).
    pub fn from_cached_records(records: &[CachedRecord]) -> Self {
        let pairs: Vec<(String, Directory)> = records
            .iter()
            .filter(|r| r.uri != "__sync__")
            .filter_map(|r| {
                let dir: Directory = serde_json::from_value(r.value.clone()).ok()?;
                Some((r.uri.clone(), dir))
            })
            .collect();
        trace!(
            "building tree from {} cached records ({} valid)",
            records.len(),
            pairs.len(),
        );
        Self::from_records(pairs)
    }

    /// Apply a delta from the AppView to cached records, returning a new set.
    ///
    /// Deleted directories are filtered out. New/updated directories replace
    /// existing records by URI. Pure function — no mutation.
    pub fn with_delta(
        records: &[CachedRecord],
        directories: &[TreeDirectory],
    ) -> Vec<CachedRecord> {
        use std::collections::HashSet;

        let deleted: HashSet<&str> = directories
            .iter()
            .filter(|d| d.deleted_at.is_some())
            .map(|d| d.directory_uri.as_str())
            .collect();

        let upserted: HashMap<&str, CachedRecord> = directories
            .iter()
            .filter(|d| d.deleted_at.is_none())
            .map(|d| (d.directory_uri.as_str(), d.to_cached_record()))
            .collect();

        let updated_uris: HashSet<&str> = upserted.keys().copied().collect();

        // Keep existing records that aren't deleted or replaced, then append upserts
        records
            .iter()
            .filter(|r| !deleted.contains(r.uri.as_str()))
            .filter(|r| !updated_uris.contains(r.uri.as_str()))
            .cloned()
            .chain(upserted.into_values())
            .collect()
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
