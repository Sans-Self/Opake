// WorkspaceAdmin: membership operations on a workspace.
//
// Thin wrapper around the federation supersede paths on `Opake`. Kept as a
// separate type for parity with `FileManager` (one Opake-bound entry per
// domain) and so the CLI / WASM bindings have a stable surface.

use crate::client::Transport;
use crate::crypto::{CryptoRng, RngCore};
use crate::error::Error;
use crate::opake::{Opake, WorkspaceMemberRemoval};
use crate::records::Role;
use crate::storage::Storage;
use crate::workspace::Workspace;

pub struct WorkspaceAdmin<'a, T: Transport, R: CryptoRng + RngCore, S: Storage> {
    pub(crate) opake: &'a mut Opake<T, R, S>,
    pub(crate) workspace: &'a Workspace,
}

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> WorkspaceAdmin<'_, T, R, S> {
    /// Resolve the recipient and return the exact unverified-key commitment
    /// that must be presented back after the owner confirms it.
    pub async fn member_approval_challenge(
        &mut self,
        member_did: &str,
    ) -> Result<Option<[u8; 32]>, Error> {
        self.opake
            .workspace_member_approval_challenge(&self.workspace.id(), member_did)
            .await
    }

    /// Add a member to the workspace via federation keyring supersede.
    ///
    /// Manager-only; authority is checked client-side then validated
    /// authoritatively at the indexer.
    pub async fn add_member(&mut self, member_did: &str, role: Role) -> Result<(), Error> {
        self.add_member_with_unverified_approval(member_did, role, None)
            .await
    }

    /// Admit a member after the caller has confirmed this exact unverified
    /// key bundle. The commitment is rechecked after fresh resolution before
    /// any wrap is emitted, so a confirmation cannot transfer to replacement
    /// keys.
    pub async fn add_member_with_unverified_approval(
        &mut self,
        member_did: &str,
        role: Role,
        confirmed_unverified_keys: Option<[u8; 32]>,
    ) -> Result<(), Error> {
        self.opake
            .add_workspace_member(
                &self.workspace.id(),
                self.workspace.current_key()?,
                &self.workspace.historical_keys,
                member_did,
                role,
                confirmed_unverified_keys,
            )
            .await?;
        Ok(())
    }

    /// Repair an admitted member's missing current wrap. An unchanged recorded
    /// approval needs no prompt; a replacement unverified bundle must carry a
    /// newly inspected confirmation token.
    pub async fn repair_member_wrap(
        &mut self,
        member_did: &str,
        confirmed_unverified_keys: Option<[u8; 32]>,
    ) -> Result<(), Error> {
        self.opake
            .repair_workspace_member_wrap(
                &self.workspace.id(),
                self.workspace.current_key()?,
                member_did,
                confirmed_unverified_keys,
            )
            .await?;
        Ok(())
    }

    /// Store a freshly confirmed unverified-key approval without attempting a
    /// wrap. This is useful when the confirming manager does not hold the
    /// current group key; a manager who does can repair later from the head.
    pub async fn approve_pending_member(
        &mut self,
        member_did: &str,
        confirmed_unverified_keys: [u8; 32],
    ) -> Result<(), Error> {
        self.opake
            .approve_pending_workspace_member(
                &self.workspace.id(),
                member_did,
                confirmed_unverified_keys,
            )
            .await?;
        Ok(())
    }

    /// Remove a member from the workspace via federation keyring supersede.
    ///
    /// Always rotates the group key. Returns `(new_group_key, new_rotation)`
    /// for the caller to cache locally — the caller's in-memory `Workspace`
    /// is now stale and should be refreshed.
    pub async fn remove_member(
        &mut self,
        member_did: &str,
    ) -> Result<WorkspaceMemberRemoval, Error> {
        self.opake
            .remove_workspace_member(
                &self.workspace.id(),
                self.workspace.current_key()?,
                member_did,
            )
            .await
    }

    /// The workspace this admin operates on.
    pub fn workspace(&self) -> &Workspace {
        self.workspace
    }
}
