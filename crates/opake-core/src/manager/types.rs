// Shared types for FileManager and WorkspaceAdmin operations.

use crate::cabinet::Cabinet;
use crate::workspace::Workspace;

/// Discriminates the crypto context for file operations.
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
pub struct UploadResult {
    /// AT-URI of the created record.
    pub uri: String,
    /// Whether the mutation was applied directly or proposed.
    pub outcome: MutationOutcome,
}

/// Result of a file download.
pub struct DownloadResult {
    pub filename: String,
    pub plaintext: Vec<u8>,
}

/// Whether a mutation was applied directly or proposed for owner approval.
///
/// Cabinet operations always return `Applied`. Workspace operations return
/// `Proposed` when the caller is a member (not the owner) — in that case,
/// a `directoryUpdate` record was written to the caller's PDS for the
/// owner's daemon to pick up.
pub enum MutationOutcome {
    /// The operation was applied directly to the PDS.
    Applied,
    /// A proposal was created for the workspace owner to apply.
    /// `update_uri` is the AT-URI of the affected entity (document or
    /// directory), not the proposal record itself.
    Proposed { update_uri: String },
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

    pub fn is_proposed(&self) -> bool {
        matches!(self, MutationOutcome::Proposed { .. })
    }
}
