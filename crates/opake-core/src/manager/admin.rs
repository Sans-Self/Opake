// WorkspaceAdmin: membership operations on a workspace.
//
// Parallel to FileManager as another Opake factory product. Handles
// add_member and leave — things that modify the keyring's member list,
// not its files.

use crate::client::Transport;
use crate::crypto::{ContentKey, CryptoRng, DidMember, PublicKeyBundle, RngCore};
use crate::error::Error;
use crate::keyrings;
use crate::opake::Opake;
use crate::records::Role;
use crate::storage::Storage;
use crate::workspace::Workspace;

pub struct WorkspaceAdmin<'a, T: Transport, R: CryptoRng + RngCore, S: Storage> {
    pub(crate) opake: &'a mut Opake<T, R, S>,
    pub(crate) workspace: &'a Workspace,
}

impl<'a, T: Transport, R: CryptoRng + RngCore, S: Storage> WorkspaceAdmin<'a, T, R, S> {
    /// Add a member to the workspace.
    ///
    /// Wraps the group key to the new member's hybrid public-key bundle and
    /// writes the updated keyring record. Only the workspace owner can call
    /// this.
    pub async fn add_member(
        &mut self,
        member_did: &str,
        member_public_keys: PublicKeyBundle<'_>,
        role: Role,
    ) -> Result<(), Error> {
        let now = self.opake.now();
        keyrings::add_member(
            &mut self.opake.client,
            &keyrings::AddMemberParams {
                keyring_uri: &self.workspace.uri,
                group_key: &self.workspace.key,
                new_member_did: member_did,
                new_member_public_keys: member_public_keys,
                role,
                modified_at: &now,
            },
            &mut self.opake.rng,
        )
        .await?;
        self.opake.auto_persist_session().await?;
        Ok(())
    }

    /// Remove a member from the workspace (rotates the workspace key).
    ///
    /// Generates a new group key, re-wraps to all remaining members, and
    /// writes the updated keyring record. Returns `(new_group_key, new_rotation)`
    /// for the caller to cache locally.
    pub async fn remove_member(
        &mut self,
        member_did: &str,
        remaining_member_keys: &[DidMember<'_>],
    ) -> Result<(ContentKey, u64), Error> {
        let now = self.opake.now();
        let result = keyrings::remove_member(
            &mut self.opake.client,
            &self.workspace.uri,
            member_did,
            remaining_member_keys,
            &self.workspace.key,
            &now,
            &mut self.opake.rng,
        )
        .await;
        self.opake.signoff(result).await
    }

    /// The workspace this admin operates on.
    pub fn workspace(&self) -> &Workspace {
        self.workspace
    }
}
