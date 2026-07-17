// In-memory snapshot of the directory hierarchy for path resolution.
//
// Loads directory records in one paginated API call. Document names are
// resolved lazily during path resolution via an async callback trait,
// so only documents in the target directory need to be fetched.

use std::collections::HashMap;

use log::trace;

use crate::atproto;
use crate::crypto::{self, DirectoryMetadata, PrivateKeyBundle};
use crate::documents::DOCUMENT_COLLECTION;
use crate::error::Error;
use crate::indexer::types::IndexerEnvelope;
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

/// Result of an incremental mutation via `apply_directory_delta`.
///
/// Consumers use this to decide whether to notify watchers and which
/// parent directories are affected (for URI-targeted watcher routing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeChange {
    /// A directory was newly added to the tree.
    Inserted { uri: String },
    /// A directory's entries or metadata changed.
    Updated { uri: String },
    /// A directory was removed from the tree.
    Removed { uri: String },
    /// The delta matched the existing state — no effective change.
    /// Watchers should skip notification.
    NoOp,
}

impl TreeChange {
    /// The URI affected by this change, if any.
    pub fn uri(&self) -> Option<&str> {
        match self {
            Self::Inserted { uri } | Self::Updated { uri } | Self::Removed { uri } => Some(uri),
            Self::NoOp => None,
        }
    }

    /// True if this change warrants firing watcher notifications.
    pub fn is_effective(&self) -> bool {
        !matches!(self, Self::NoOp)
    }
}

/// Decryption context for applying SSE events to an in-memory tree.
///
/// Provides the keys needed to decrypt directory names. Cabinet trees use
/// `private_keys` (the caller's hybrid bundle) for direct key wrapping;
/// workspace trees use `group_keys` (keyring URI → rotation-aware key view)
/// for keyring wrapping. Trees can hold directories of either kind, so both
/// fields may be needed on the same context.
#[derive(Debug)]
pub struct DecryptionCtx<'a> {
    pub did: &'a str,
    pub private_keys: Option<&'a PrivateKeyBundle<'a>>,
    pub group_keys: &'a HashMap<String, crate::workspace::GroupKeys<'a>>,
}

impl<'a> DecryptionCtx<'a> {
    pub fn cabinet(did: &'a str, private_keys: &'a PrivateKeyBundle<'a>) -> Self {
        Self {
            did,
            private_keys: Some(private_keys),
            group_keys: EMPTY_GROUP_KEYS.get_or_init(HashMap::new),
        }
    }

    pub fn workspace(
        did: &'a str,
        group_keys: &'a HashMap<String, crate::workspace::GroupKeys<'a>>,
    ) -> Self {
        Self {
            did,
            private_keys: None,
            group_keys,
        }
    }
}

static EMPTY_GROUP_KEYS: std::sync::OnceLock<
    HashMap<String, crate::workspace::GroupKeys<'static>>,
> = std::sync::OnceLock::new();

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
    /// The directory record's own declared `opakeVersion`. Threaded into the
    /// content-key unwrap so the KDF transcript derives from the record's
    /// declaration rather than this client's compile-time constant (see
    /// `record-validity` § cryptographic parameters derive from the record's
    /// declaration).
    opake_version: u32,
    key_wrapping: KeyWrapping,
    encrypted_metadata: EncryptedMetadata,
    entries: Vec<String>,
    /// AT-URI of the directory this record supersedes, if any.
    supersedes_uri: Option<String>,
    /// This chain's genesis URI, if the record declared one. Absent on a
    /// genesis (or cabinet) record. The metadata AAD binds the lineage
    /// anchor `lineage.unwrap_or(own_uri)`.
    lineage: Option<String>,
    /// `true` iff this record is part of a workspace-root chain. Stamped
    /// by writers; the indexer enforces the flag never flips across a
    /// supersede. Used to detect the workspace root at bootstrap (the
    /// flagged record with no successor).
    is_workspace_root: bool,
}

/// Client-assigned display name for a placeholder standing in for a corrupt or
/// future-version directory record. NEVER derived from record content — the
/// record is exactly the thing we could not read (see `record-validity`
/// § corrupt containers render as placeholders).
pub const PLACEHOLDER_DISPLAY_NAME: &str = "Unreadable item";

/// An opaque stand-in for a directory record the client could not fully
/// understand, rendered at the position surviving references establish for it.
///
/// A placeholder is derived state, held apart from `directories` so it can never
/// be confused with a readable record: it carries no crypto envelope and no
/// attacker-controllable text. Its `entries` are whatever children a prior
/// readable version of the record listed — preserved across a degrade so the
/// subtree stays attached, empty when the record was corrupt from first sight.
#[derive(Debug, Clone)]
pub struct PlaceholderNode {
    /// Why the underlying record is unreadable.
    pub reason: crate::records::UnreadableReason,
    /// Child URIs preserved from a prior readable version, if any.
    pub entries: Vec<String>,
}

/// Outcome of folding unreadable references into the tree: how many surfaced as
/// visible placeholders vs how many were count-only (no URI, a non-directory,
/// or an element the authorized snapshot does not reference).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnreadableTally {
    pub placeholders: usize,
    pub count_only: usize,
}

impl UnreadableTally {
    /// Total unreadable references folded in.
    pub fn total(&self) -> usize {
        self.placeholders + self.count_only
    }
}

#[derive(Debug)]
pub struct DirectoryTree {
    /// URI → (name, entries) for every directory record.
    directories: HashMap<String, DirectoryInfo>,
    /// URI → placeholder for every corrupt / future-version directory record
    /// the authorized snapshot references. Disjoint from `directories`: a
    /// readable record always wins, so a URI is never in both.
    placeholders: HashMap<String, PlaceholderNode>,
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
                        opake_version: dir.opake_version,
                        key_wrapping: dir.key_wrapping,
                        encrypted_metadata: dir.encrypted_metadata,
                        entries: dir.entries.into_iter().map(|e| e.target).collect(),
                        supersedes_uri: dir.supersedes,
                        lineage: dir.lineage,
                        is_workspace_root: dir.is_workspace_root,
                    },
                )
            })
            .collect();

        // Two root-detection rules:
        //   * Cabinet: the directory at rkey "self".
        //   * Workspace: the directory in the workspace-root chain that
        //     has no successor superseding it. We project to chain heads
        //     by finding records whose URI doesn't appear as anyone
        //     else's `supersedes_uri`, then keep the one with
        //     `is_workspace_root: true`.
        // A supersede edge only counts when the superseding record's declared
        // lineage equals its predecessor's anchor. A flipped-lineage record is
        // outside the chain, so its edge is dropped read-leniently and the
        // prior it names stays a head
        // (`spec:lineage § Lineage never flips across a supersede`).
        let superseded_uris: std::collections::HashSet<&str> = directories
            .iter()
            .filter_map(|(uri, info)| {
                let prior_uri = info.supersedes_uri.as_deref()?;
                if let Some(prior) = directories.get(prior_uri) {
                    let prior_anchor = prior.lineage.as_deref().unwrap_or(prior_uri);
                    if info.lineage.as_deref().unwrap_or(uri) != prior_anchor {
                        return None;
                    }
                }
                Some(prior_uri)
            })
            .collect();

        let root_uri = directories
            .iter()
            .find(|(uri, info)| info.is_workspace_root && !superseded_uris.contains(uri.as_str()))
            .map(|(uri, _)| uri.clone())
            .or_else(|| {
                directories
                    .keys()
                    .find(|uri| {
                        atproto::parse_at_uri(uri)
                            .map(|u| u.rkey == ROOT_DIRECTORY_RKEY)
                            .unwrap_or(false)
                    })
                    .cloned()
            });

        trace!(
            "built tree: {} directories, root={}",
            directories.len(),
            root_uri.as_deref().unwrap_or("none"),
        );

        Self {
            directories,
            placeholders: HashMap::new(),
            root_uri,
        }
    }

    /// Override the root directory URI.
    ///
    /// Used for workspace trees where the root is not `directory/self` but
    /// a deterministic `directory/ws-{keyring_rkey}`. Stores the URI
    /// unconditionally — the tree records "this is the expected root"
    /// even if the corresponding directory record hasn't landed yet
    /// (cold indexer window, SSE still catching up). When the record
    /// does arrive via SSE upsert, it slots into `directories` under
    /// this URI and the tree becomes fully populated. Gating this on
    /// `contains_key` made the workspace root silently unmarked during
    /// the load→install→first-SSE-event window, which surfaced as
    /// `snapshot.rootUri === undefined` in the JS consumer.
    ///
    /// Walks the supersede chain forward: callers pass the deterministic
    /// genesis URI, and this method advances `root_uri` to the head of
    /// the chain by following any record whose `supersedes_uri` points at
    /// the current candidate. Necessary at bootstrap because the indexer
    /// returns every record in the chain, not just the head — without the
    /// walk, `root_uri` would pin to the (immutable, superseded) genesis
    /// and stale entries would leak into the snapshot. Cycle-safe: bounded
    /// by `directories.len()` iterations.
    pub fn set_root(&mut self, uri: &str) {
        let mut current = uri.to_owned();
        let bound = self.directories.len().saturating_add(1);
        for _ in 0..bound {
            // Anchor of the current record — a successor must declare it as its
            // lineage, or it is outside the chain and the walk stops here
            // (`spec:lineage § Lineage never flips across a supersede`).
            let current_anchor = self
                .directories
                .get(&current)
                .and_then(|info| info.lineage.clone())
                .unwrap_or_else(|| current.clone());
            let next = self
                .directories
                .iter()
                .find(|(succ_uri, info)| {
                    info.supersedes_uri.as_deref() == Some(current.as_str())
                        && info.lineage.as_deref().unwrap_or(succ_uri) == current_anchor
                })
                .map(|(succ_uri, _)| succ_uri.clone());
            match next {
                Some(succ) => current = succ,
                None => break,
            }
        }
        self.root_uri = Some(current);
    }

    /// Load the directory hierarchy from the PDS (test use only).
    ///
    /// Production code uses Indexer snapshots via `from_cached_records()`.
    #[cfg(test)]
    pub(crate) async fn load(
        client: &mut crate::client::XrpcClient<impl crate::client::Transport>,
    ) -> Result<Self, Error> {
        let dir_entries: Vec<(String, Directory)> = crate::client::list_collection(
            client,
            DIRECTORY_COLLECTION,
            crate::records::vocabulary::RecordKind::Directory,
            crate::client::DegradationPolicy::Counted,
            |uri, dir: Directory, _needs_newer| (uri.to_owned(), dir),
        )
        .await?
        .entries;

        Ok(Self::from_records(dir_entries))
    }

    /// Decrypt all directory names in-place.
    ///
    /// Unwraps each directory's content key from the encryption envelope,
    /// then decrypts the metadata to recover the real name. Directories
    /// whose keys can't be unwrapped (wrong DID, keyring not available)
    /// get a fallback name of "?".
    pub fn decrypt_names(&mut self, did: &str, private_keys: &PrivateKeyBundle<'_>) {
        self.decrypt_names_with_group_keys(did, private_keys, &HashMap::new());
    }

    /// Decrypt all directory names in-place, with rotation-aware group key
    /// support.
    ///
    /// `group_keys` maps keyring URI → all group keys the caller had access
    /// to (current + historical). For each keyring-encrypted directory, the
    /// rotation embedded in `keyringRef.rotation` selects the right key.
    pub fn decrypt_names_with_group_keys(
        &mut self,
        did: &str,
        private_keys: &PrivateKeyBundle<'_>,
        group_keys: &HashMap<String, crate::workspace::GroupKeys<'_>>,
    ) {
        for (uri, info) in self.directories.iter_mut() {
            let anchor = info.lineage.clone().unwrap_or_else(|| uri.clone());
            let content_key = match &info.key_wrapping {
                KeyWrapping::Direct(direct) => {
                    // Direct-wrapped directories live in the cabinet (workspace
                    // dirs always go through the keyring path). The original
                    // wrap was scoped with `WrapContext::Cabinet`.
                    let wrapped = direct.keys.iter().find(|k| k.did == did);
                    match wrapped {
                        Some(w) => crypto::unwrap_key(
                            w,
                            private_keys,
                            &crypto::WrapContext::Cabinet,
                            info.opake_version,
                        )
                        .ok(),
                        None => None,
                    }
                }
                KeyWrapping::Keyring(kr) => {
                    let keyring_uri = &kr.keyring_ref.keyring;
                    let dir_rotation = kr.keyring_ref.rotation;
                    group_keys.get(keyring_uri).and_then(|keys| {
                        let gk = keys.for_rotation(dir_rotation)?;
                        let wrapped_bytes = kr.keyring_ref.wrapped_content_key.decode().ok()?;
                        crypto::unwrap_content_key_from_keyring(&wrapped_bytes, gk).ok()
                    })
                }
            };

            if let Some(key) = content_key {
                let context =
                    crypto::SealContext::new(&anchor, crypto::SealType::DirectoryMetadata);
                if let Ok(metadata) = crypto::decrypt_metadata::<DirectoryMetadata>(
                    &key,
                    &info.encrypted_metadata,
                    &context,
                ) {
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
                        .directory_name(uri.as_str())
                        .map(str::to_owned)
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

        dirs.sort_by_key(|a| a.2.to_lowercase());
        docs.sort_by_key(|a| a.2.to_lowercase());
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
                if let Some(entries) = self.entries_for(uri.as_str()) {
                    let child_prefix = if is_last {
                        format!("{prefix}    ")
                    } else {
                        format!("{prefix}│   ")
                    };
                    let sorted = self.sort_entries(entries, documents);
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
    /// is not a known directory. A placeholder yields its preserved children
    /// (empty when it was corrupt from first sight).
    pub fn entries_for(&self, uri: &str) -> Option<&[String]> {
        self.directories
            .get(uri)
            .map(|info| info.entries.as_slice())
            .or_else(|| self.placeholders.get(uri).map(|p| p.entries.as_slice()))
    }

    /// Returns the decrypted name for a directory URI. A placeholder returns
    /// the client-assigned [`PLACEHOLDER_DISPLAY_NAME`], never record content.
    pub fn directory_name(&self, uri: &str) -> Option<&str> {
        if let Some(info) = self.directories.get(uri) {
            return Some(info.name.as_str());
        }
        self.placeholders.get(uri).map(|_| PLACEHOLDER_DISPLAY_NAME)
    }

    /// Whether the given URI is a known directory in this tree — a readable
    /// record or a placeholder standing in for an unreadable one.
    pub fn is_directory(&self, uri: &str) -> bool {
        self.directories.contains_key(uri) || self.placeholders.contains_key(uri)
    }

    /// Whether the given URI is a placeholder for an unreadable record.
    pub fn is_placeholder(&self, uri: &str) -> bool {
        self.placeholders.contains_key(uri)
    }

    /// The unreadable reason for a placeholder URI, if it is one.
    pub fn placeholder_reason(&self, uri: &str) -> Option<crate::records::UnreadableReason> {
        self.placeholders.get(uri).map(|p| p.reason)
    }

    /// URIs of every placeholder node currently in the tree.
    pub fn placeholder_uris(&self) -> impl Iterator<Item = &str> {
        self.placeholders.keys().map(String::as_str)
    }

    /// Whether the given URI looks like a document URI (by collection segment).
    pub fn is_document_uri(&self, uri: &str) -> bool {
        entry_kind_from_uri(uri) == Some(EntryKind::Document)
    }

    /// Iterate over **every** directory record's URI, including superseded
    /// predecessors. The indexer snapshot returns whole supersede chains, so
    /// this yields stale records too — correct only for whole-chain
    /// operations. For anything that treats the result as the *live* tree
    /// (snapshot building, parent/child indexing), use
    /// [`Self::canonical_directory_uris`] instead.
    pub fn all_directory_uris(&self) -> impl Iterator<Item = &str> {
        self.directories.keys().map(String::as_str)
    }

    /// Directory URIs in the current canonical tree — chain heads only.
    /// Superseded predecessors are excluded. This is the set the live tree
    /// view should be built from.
    pub fn canonical_directory_uris(&self) -> Vec<&str> {
        let superseded = self.superseded_uris();
        self.directories
            .keys()
            .filter(|uri| !superseded.contains(uri.as_str()))
            .map(String::as_str)
            .collect()
    }

    /// URIs that some other directory record supersedes — i.e. the stale
    /// predecessors. Computed from the `supersedes` back-edges present in the
    /// record set. A back-edge only counts when the superseding record's
    /// declared lineage equals its predecessor's anchor; a flipped-lineage
    /// record is outside the chain, so its edge is dropped read-leniently and
    /// the prior it names stays canonical
    /// (`spec:lineage § Lineage never flips across a supersede`).
    fn superseded_uris(&self) -> std::collections::HashSet<&str> {
        self.directories
            .iter()
            .filter_map(|(uri, info)| {
                let prior_uri = info.supersedes_uri.as_deref()?;
                if let Some(prior) = self.directories.get(prior_uri) {
                    let prior_anchor = prior.lineage.as_deref().unwrap_or(prior_uri);
                    if info.lineage.as_deref().unwrap_or(uri) != prior_anchor {
                        return None;
                    }
                }
                Some(prior_uri)
            })
            .collect()
    }

    /// Count descendant documents and directories under a directory URI.
    ///
    /// A hostile or buggy writer can publish a cyclic listing; `expanded`
    /// bounds the walk so each directory contributes its entries once.
    pub fn count_descendants(&self, uri: &str) -> (usize, usize) {
        let mut documents = 0usize;
        let mut directories = 0usize;
        let mut expanded: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut stack = vec![uri.to_owned()];

        while let Some(current) = stack.pop() {
            if !expanded.insert(current.clone()) {
                continue;
            }
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
    ///
    /// `expanded` bounds the walk on a cyclic listing: the domain API refuses
    /// to write one, but a hostile or buggy writer can, and a consumer that
    /// re-expands a cycle member never terminates. Each directory is expanded
    /// once, so a cycle yields the reachable set and the walk ends.
    pub fn collect_descendants(&self, uri: &str) -> Vec<(String, EntryKind)> {
        let mut result = Vec::new();
        let mut expanded: std::collections::HashSet<String> = std::collections::HashSet::new();
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

            if !expanded.insert(current.clone()) {
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

    /// Find the canonical directory that currently lists `child_uri`.
    ///
    /// Only chain-head directories are considered. The indexer snapshot
    /// returns the entire directory chain — every superseded predecessor as
    /// well as the current head — and a child that predates its parent's
    /// latest supersede is listed by *every* prior version of that parent.
    /// Scanning all records would therefore return an arbitrary (possibly
    /// superseded) parent, and walking up from it lands on a superseded root
    /// that no longer matches the indexer's chain head. Skipping any record
    /// that something else supersedes leaves exactly the canonical parent.
    pub fn find_parent(&self, child_uri: &str) -> Option<String> {
        let superseded = self.superseded_uris();

        for (dir_uri, info) in &self.directories {
            if superseded.contains(dir_uri.as_str()) {
                continue; // Superseded predecessor — not part of the live tree.
            }
            if info.entries.iter().any(|e| e == child_uri) {
                return Some(dir_uri.clone());
            }
        }
        None
    }

    /// True if `uri` is the root directory or chains up to it via parent
    /// links. After a delete orphans a subtree — the deleted directory's
    /// record is gone, so its children no longer have a parent in the
    /// tree — those descendants return `false`. Used to auto-close
    /// watchers stranded on a directory that can no longer be reached
    /// from the root.
    pub fn is_reachable_from_root(&self, uri: &str) -> bool {
        let Some(root) = self.root_uri.as_deref() else {
            return false;
        };
        if uri == root {
            return true;
        }
        let mut current = uri.to_string();
        // Bounded by the directory count: a detached chain or cycle can't
        // visit more distinct nodes than exist, so this always terminates.
        for _ in 0..=self.directories.len() {
            match self.find_parent(&current) {
                Some(parent) if parent == root => return true,
                Some(parent) => current = parent,
                None => return false,
            }
        }
        false
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

    // -----------------------------------------------------------------------
    // Incremental mutation (for SSE-driven tree patching)
    // -----------------------------------------------------------------------

    /// Remove a directory record by URI. If the deleted URI was the root,
    /// clear `root_uri`. Also clears any placeholder standing in for the URI.
    /// Returns the resulting tree change.
    pub fn apply_directory_delete(&mut self, uri: &str) -> TreeChange {
        let removed_record = self.directories.remove(uri).is_some();
        let removed_placeholder = self.placeholders.remove(uri).is_some();
        if !removed_record && !removed_placeholder {
            return TreeChange::NoOp;
        }
        if self.root_uri.as_deref() == Some(uri) {
            self.root_uri = None;
        }
        TreeChange::Removed {
            uri: uri.to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // Degradation — placeholders for corrupt / future-version records
    // -----------------------------------------------------------------------

    /// Fold a batch of unreadable references (from lenient snapshot/delta
    /// classification) into the tree. Returns a tally of how many became
    /// visible placeholders vs how many were count-only.
    ///
    /// Idempotent and position-stable: re-applying the same refs across a
    /// snapshot refresh preserves each placeholder at the position its
    /// surviving references establish (see `record-validity` § corrupt
    /// containers render as placeholders).
    pub fn apply_unreadable_refs(
        &mut self,
        refs: &[crate::records::UnreadableRef],
    ) -> UnreadableTally {
        let mut tally = UnreadableTally::default();
        for reference in refs {
            if self.apply_unreadable_ref(reference) {
                tally.placeholders += 1;
            } else {
                tally.count_only += 1;
            }
        }
        tally
    }

    /// Fold one unreadable reference in. Returns `true` iff it became a visible
    /// placeholder — the reference must carry a URI, name a directory record,
    /// and be referenced by the authorized snapshot. Everything else is
    /// count-only: a missing URI, a corrupt document (no node shape), or an
    /// element nothing in scope references (no out-of-scope disclosure).
    ///
    /// When the URI currently renders as a readable directory, it is degraded
    /// in place — its children move onto the placeholder so the subtree stays
    /// attached and visible beneath it.
    pub fn apply_unreadable_ref(&mut self, reference: &crate::records::UnreadableRef) -> bool {
        let Some(uri) = reference.uri.as_deref() else {
            return false; // count-only: no URI to hang a node on
        };
        // Only directory records are placeholder containers; a corrupt document
        // has no children and no node to stand in for.
        if entry_kind_from_uri(uri) != Some(EntryKind::Directory) {
            return false;
        }
        // Authorized-snapshot invariant: only elements the snapshot already
        // references may surface. Degradation never invents or moves a position.
        if !self.is_referenced(uri) {
            return false;
        }

        // Preserve children: from a readable record being degraded, or from a
        // prior placeholder at this URI. Empty when the record was corrupt from
        // first sight.
        let entries = self
            .directories
            .remove(uri)
            .map(|info| info.entries)
            .or_else(|| self.placeholders.get(uri).map(|p| p.entries.clone()))
            .unwrap_or_default();

        self.placeholders.insert(
            uri.to_string(),
            PlaceholderNode {
                reason: reference.reason,
                entries,
            },
        );
        true
    }

    /// Whether any readable directory or placeholder lists `uri` as a child.
    fn is_referenced(&self, uri: &str) -> bool {
        self.directories
            .values()
            .any(|d| d.entries.iter().any(|e| e == uri))
            || self
                .placeholders
                .values()
                .any(|p| p.entries.iter().any(|e| e == uri))
    }

    /// Apply a single indexed directory record to the in-memory tree.
    ///
    /// SSE-driven consumers call this per event to keep a persistent
    /// tree in sync without rebuilding from scratch. `uri` identifies
    /// the record (at-uri); `dir` is the verbatim PDS record.
    ///
    /// The record's name is decrypted in place using `ctx`. If
    /// decryption fails (wrong key, missing keyring), the name falls
    /// back to `"?"`.
    pub fn apply_directory_delta(
        &mut self,
        uri: &str,
        dir: &Directory,
        ctx: &DecryptionCtx<'_>,
    ) -> Result<TreeChange, Error> {
        let existed = self.directories.contains_key(uri);

        // Project the record into the tree's internal shape. Entries become
        // bare target URIs (the cascade-pinned CIDs live on the record but
        // aren't load-bearing for in-memory navigation).
        let entries: Vec<String> = dir.entries.iter().map(|e| e.target.clone()).collect();

        let mut info = DirectoryInfo {
            name: String::new(),
            opake_version: dir.opake_version,
            key_wrapping: dir.key_wrapping.clone(),
            encrypted_metadata: dir.encrypted_metadata.clone(),
            entries,
            supersedes_uri: dir.supersedes.clone(),
            lineage: dir.lineage.clone(),
            is_workspace_root: dir.is_workspace_root,
        };

        info.name = decrypt_directory_name(uri, &info, ctx).unwrap_or_else(|| "?".into());

        // A readable record supersedes any placeholder that stood in for this
        // URI — the record is understood now, so the stand-in is gone.
        self.placeholders.remove(uri);
        self.directories.insert(uri.to_string(), info);

        // Advance `root_uri` when this record supersedes the current root,
        // OR when it carries the `isWorkspaceRoot` flag and we have no root
        // set yet. The flag is authoritative — the indexer rejects supersedes
        // that flip it, so seeing it true is sufficient.
        if let Some(prior) = dir.supersedes.as_deref() {
            if self.root_uri.as_deref() == Some(prior) {
                self.root_uri = Some(uri.to_string());
                if let Some(entry) = self.directories.get_mut(uri) {
                    entry.name = ROOT_DIRECTORY_NAME.into();
                }
            }
        }

        if self.root_uri.is_none() {
            // Cabinet: rkey "self" is the root by convention.
            // Workspace: the writer stamps `isWorkspaceRoot: true` on every
            // record in the root chain.
            let is_root = dir.is_workspace_root
                || atproto::parse_at_uri(uri)
                    .map(|u| u.rkey == ROOT_DIRECTORY_RKEY)
                    .unwrap_or(false);
            if is_root {
                self.root_uri = Some(uri.to_string());
                if let Some(entry) = self.directories.get_mut(uri) {
                    entry.name = ROOT_DIRECTORY_NAME.into();
                }
            }
        }

        if existed {
            Ok(TreeChange::Updated {
                uri: uri.to_string(),
            })
        } else {
            Ok(TreeChange::Inserted {
                uri: uri.to_string(),
            })
        }
    }

    /// Clear cached decrypted names. Used after a keyring rotation,
    /// since the content keys that produced the names are now stale.
    /// Subsequent reads will show `"?"` until the tree is re-decrypted
    /// (either via a full `decrypt_names` pass or per-directory apply).
    pub fn invalidate_decrypted_names(&mut self) {
        for info in self.directories.values_mut() {
            info.name.clear();
        }
    }

    /// Apply a delta from the indexer to cached records, returning a new set.
    ///
    /// Deleted directories are filtered out. New/updated directories replace
    /// existing records by URI. Pure function — no mutation.
    pub fn with_delta(
        records: &[CachedRecord],
        envelopes: &[IndexerEnvelope<Directory>],
    ) -> Vec<CachedRecord> {
        use std::collections::HashSet;

        let deleted: HashSet<&str> = envelopes
            .iter()
            .filter(|e| e.deleted_at.is_some())
            .map(|e| e.uri.as_str())
            .collect();

        let upserted: HashMap<&str, CachedRecord> = envelopes
            .iter()
            .filter(|e| e.deleted_at.is_none())
            .filter_map(|e| {
                let value = serde_json::to_value(&e.record).ok()?;
                Some((
                    e.uri.as_str(),
                    CachedRecord {
                        uri: e.uri.clone(),
                        cid: String::new(),
                        value,
                    },
                ))
            })
            .collect();

        let updated_uris: HashSet<&str> = upserted.keys().copied().collect();

        records
            .iter()
            .filter(|r| !deleted.contains(r.uri.as_str()))
            .filter(|r| !updated_uris.contains(r.uri.as_str()))
            .cloned()
            .chain(upserted.into_values())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Incremental-apply helpers
// ---------------------------------------------------------------------------

/// Decrypt a single directory's name using the decryption context.
///
/// Mirrors the logic in `decrypt_names_with_group_keys` but operates on
/// one directory at a time. Returns `None` if the key can't be unwrapped
/// or the metadata can't be decrypted — the caller falls back to `"?"`.
fn decrypt_directory_name(
    uri: &str,
    info: &DirectoryInfo,
    ctx: &DecryptionCtx<'_>,
) -> Option<String> {
    let content_key = match &info.key_wrapping {
        KeyWrapping::Direct(direct) => {
            let wrapped = direct.keys.iter().find(|k| k.did == ctx.did)?;
            let private_keys = ctx.private_keys?;
            crypto::unwrap_key(
                wrapped,
                private_keys,
                &crypto::WrapContext::Cabinet,
                info.opake_version,
            )
            .ok()?
        }
        KeyWrapping::Keyring(kr) => {
            let keyring_uri = &kr.keyring_ref.keyring;
            let dir_rotation = kr.keyring_ref.rotation;
            let keys = ctx.group_keys.get(keyring_uri)?;
            let group_key = keys.for_rotation(dir_rotation)?;
            let wrapped_bytes = kr.keyring_ref.wrapped_content_key.decode().ok()?;
            crypto::unwrap_content_key_from_keyring(&wrapped_bytes, group_key).ok()?
        }
    };

    let anchor = info.lineage.as_deref().unwrap_or(uri);
    let context = crypto::SealContext::new(anchor, crypto::SealType::DirectoryMetadata);
    crypto::decrypt_metadata::<DirectoryMetadata>(&content_key, &info.encrypted_metadata, &context)
        .ok()
        .map(|meta| meta.name)
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
