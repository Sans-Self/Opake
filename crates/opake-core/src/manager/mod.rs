// FileManager: unified file operations for cabinet and workspace contexts.
//
// The FileManager is the primary public API for opake-core file operations.
// It dispatches internally based on the FileContext (Cabinet vs Workspace),
// handling encryption differences and the owner/member proposal pattern
// transparently.
//
// Construct via `Opake::file_manager()`.

mod admin;
mod delete;
mod directory;
mod download;
mod editor;
mod move_entry;
mod rename;
mod sharing;
mod tree;
mod types;
mod upload;

pub use admin::WorkspaceAdmin;
pub use types::{
    DownloadResult, FileContext, MutationOutcome, ResolvedDocumentMetadata, UploadRequest,
    UploadResult,
};

use crate::client::Transport;
use crate::crypto::{CryptoRng, RngCore};
use crate::indexer::{DocumentProposal, KeyringProposal, TreeProposal};
use crate::opake::Opake;
use crate::storage::Storage;

pub struct FileManager<'a, T: Transport, R: CryptoRng + RngCore, S: Storage> {
    pub(crate) opake: &'a mut Opake<T, R, S>,
    pub(crate) context: &'a FileContext,
    /// Directory proposals from the last tree sync (workspace only).
    pub(crate) last_proposals: Vec<TreeProposal>,
    /// Keyring proposals from the last tree sync (workspace only).
    pub(crate) last_keyring_proposals: Vec<KeyringProposal>,
    /// Document update proposals from the last tree sync (workspace only).
    pub(crate) last_document_proposals: Vec<DocumentProposal>,
}

impl<'a, T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'a, T, R, S> {
    /// Whether the caller is the owner of the file context.
    ///
    /// Cabinet: always `true` (the user owns their own files).
    /// Workspace: `true` if the caller's DID matches the workspace owner.
    pub fn is_owner(&self) -> bool {
        self.opake.did == self.context.owner_did()
    }

    /// The file context (Cabinet or Workspace).
    pub fn context(&self) -> &FileContext {
        self.context
    }

    /// Pending directory update proposals from the last tree sync.
    ///
    /// Only populated for workspace contexts after `load_tree()`. The Indexer
    /// verifies that each proposal's author is a current workspace member.
    pub fn proposals(&self) -> &[TreeProposal] {
        &self.last_proposals
    }

    /// Pending keyring update proposals from the last tree sync.
    pub fn last_keyring_proposals(&self) -> &[KeyringProposal] {
        &self.last_keyring_proposals
    }

    /// Pending document update proposals from the last tree sync.
    pub fn last_document_proposals(&self) -> &[DocumentProposal] {
        &self.last_document_proposals
    }

    /// Create a record on the caller's PDS.
    ///
    /// Low-level passthrough for one-off writes that don't fit other
    /// FileManager methods (e.g., pending shares).
    pub async fn create_record(
        &mut self,
        collection: &str,
        record: &impl serde::Serialize,
    ) -> Result<crate::client::RecordRef, crate::error::Error> {
        let result = self.opake.client.create_record(collection, record).await;
        self.opake.signoff(result).await
    }
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
