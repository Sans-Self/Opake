// Shared types for FileManager and WorkspaceAdmin operations.

use crate::cabinet::Cabinet;
use crate::workspace::Workspace;

/// Discriminates the crypto context for file operations.
// Cabinet's key material makes the variant ~3.7KB. FileContext values are
// created once per operation and never collected, so the stack size is
// irrelevant and boxing would only add indirection to every match.
#[allow(clippy::large_enum_variant)]
pub enum FileContext {
    /// Personal file space — direct (asymmetric) key wrapping.
    Cabinet(Cabinet),
    /// Shared file space — keyring (symmetric) key wrapping.
    Workspace(Workspace),
}

impl FileContext {
    /// DID of the record owner. For cabinet, this is the caller.
    /// For workspace, this is the workspace owner (may differ from caller).
    pub fn owner_did(&self) -> &str {
        match self {
            FileContext::Cabinet(c) => &c.did,
            FileContext::Workspace(w) => &w.owner_did,
        }
    }

    pub fn is_workspace(&self) -> bool {
        matches!(self, FileContext::Workspace(_))
    }

    pub fn is_cabinet(&self) -> bool {
        matches!(self, FileContext::Cabinet(_))
    }
}

/// Parameters for uploading a file. Crypto details (which key, how to wrap)
/// are determined by the FileManager's context — the caller just provides
/// the plaintext and metadata.
pub struct UploadRequest<'a> {
    pub plaintext: &'a [u8],
    pub filename: &'a str,
    pub mime_type: &'a str,
    pub description: Option<&'a str>,
    pub tags: &'a [String],
    /// Target directory URI. `None` means the root directory.
    pub directory_uri: Option<&'a str>,
}

/// Result of an upload or directory creation.
#[derive(Debug, Clone)]
pub struct UploadResult {
    /// AT-URI of the created record.
    pub uri: String,
    /// Outcome marker for the cascade. Kept as a typed enum (rather than
    /// `()`) so future variants — `Reused` for a deduped genesis, `Skipped`
    /// for a no-op — can land without rippling through the call sites.
    pub outcome: MutationOutcome,
}

/// Result of a file download.
pub struct DownloadResult {
    pub filename: String,
    pub plaintext: Vec<u8>,
}

/// Whether a mutation was applied directly.
///
/// Pre-federation this enum also carried a `Proposed` variant for editor
/// writes that targeted someone else's PDS. The federation rewrite replaces
/// those with curatorial-supersede cascades — every chain participant
/// writes to their own PDS, so every mutation reaches `Applied`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationOutcome {
    /// The operation was applied directly to the PDS.
    Applied,
}

/// Document metadata including record timestamps.
///
/// Combines the decrypted metadata (from the encrypted envelope) with
/// the unencrypted timestamps from the PDS record.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResolvedDocumentMetadata {
    pub name: String,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
    pub tags: Vec<String>,
    pub description: Option<String>,
    pub created_at: String,
    pub modified_at: Option<String>,
}

impl ResolvedDocumentMetadata {
    /// Build from decrypted metadata + record timestamps.
    pub fn from_parts(
        meta: crate::crypto::DocumentMetadata,
        created_at: String,
        modified_at: Option<String>,
    ) -> Self {
        Self {
            name: meta.name,
            mime_type: meta.mime_type,
            size: meta.size,
            tags: meta.tags,
            description: meta.description,
            created_at,
            modified_at,
        }
    }
}

impl MutationOutcome {
    pub fn is_applied(&self) -> bool {
        matches!(self, MutationOutcome::Applied)
    }
}
