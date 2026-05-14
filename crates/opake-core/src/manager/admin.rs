// WorkspaceAdmin: membership operations on a workspace.
//
// Thin wrapper around the federation supersede paths on `Opake`. Kept as a
// separate type for parity with `FileManager` (one Opake-bound entry per
// domain) and so the CLI / WASM bindings have a stable surface.

use crate::client::Transport;
use crate::crypto::{ContentKey, CryptoRng, RngCore};
use crate::error::Error;
use crate::opake::Opake;
use crate::records::Role;
use crate::storage::Storage;
use crate::workspace::Workspace;

pub struct WorkspaceAdmin<'a, T: Transport, R: CryptoRng + RngCore, S: Storage> {
    pub(crate) opake: &'a mut Opake<T, R, S>,
    pub(crate) workspace: &'a Workspace,
}

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> WorkspaceAdmin<'_, T, R, S> {
    /// Add a member to the workspace via federation keyring supersede.
    ///
    /// Manager-only; authority is checked client-side then validated
    /// authoritatively at the indexer.
    pub async fn add_member(&mut self, member_did: &str, role: Role) -> Result<(), Error> {
        self.opake
            .add_workspace_member(&self.workspace.uri, &self.workspace.key, member_did, role)
            .await?;
        Ok(())
    }

    /// Remove a member from the workspace via federation keyring supersede.
    ///
    /// Always rotates the group key. Returns `(new_group_key, new_rotation)`
    /// for the caller to cache locally — the caller's in-memory `Workspace`
    /// is now stale and should be refreshed.
    pub async fn remove_member(&mut self, member_did: &str) -> Result<(ContentKey, u64), Error> {
        self.opake
            .remove_workspace_member(&self.workspace.uri, &self.workspace.key, member_did)
            .await
    }

    /// The workspace this admin operates on.
    pub fn workspace(&self) -> &Workspace {
        self.workspace
    }
}
