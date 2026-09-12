use crate::client::Transport;
use crate::crypto::{CryptoRng, PublicKeyBundle, RngCore};
use crate::documents;
use crate::error::Error;
use crate::resolve::{ResolvedIdentity, VerificationState};
use crate::sharing::{self, GrantEntry, GrantParams};
use crate::storage::Storage;

use super::types::FileContext;
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Inspect the exact approval required to share with an unverified
    /// resolved recipient. `None` means their DID document verified the
    /// published encryption bundle.
    pub fn share_approval_challenge(
        &self,
        document_uri: &str,
        recipient: &ResolvedIdentity,
    ) -> Option<[u8; 32]> {
        match recipient.verification {
            VerificationState::Verified { .. } => None,
            VerificationState::Unverified => Some(recipient.unverified_key_approval(document_uri)),
        }
    }

    /// Share a document with a freshly resolved recipient DID by creating a grant.
    ///
    /// Cabinet only. Fetches the document's content key, wraps it to the
    /// recipient's hybrid public-key bundle, and creates a grant record.
    ///
    /// Returns the grant AT-URI.
    #[::opake_derive::signoff]
    pub async fn share(
        &mut self,
        document_uri: &str,
        recipient_did: &str,
        confirmed_unverified_keys: Option<[u8; 32]>,
        permissions: &str,
        note: Option<&str>,
    ) -> Result<String, Error> {
        let FileContext::Cabinet(ref cabinet) = self.context else {
            return Err(Error::InvalidRecord(
                "sharing is only supported from the cabinet".into(),
            ));
        };

        // Inspection returns a displayable challenge, but it is never the
        // mutation authority. Resolve again at the last responsible moment so
        // a DID-document anchor, signature, or key replacement change blocks
        // the grant before its content key is fetched or wrapped.
        let recipient = self.opake.resolve_identity(recipient_did).await?;

        // The confirmation is checked against this exact, freshly resolved
        // bundle. A stale token cannot survive key substitution.
        let unverified_key_approval = match recipient.verification {
            VerificationState::Verified { .. } => None,
            VerificationState::Unverified => {
                let expected = recipient.unverified_key_approval(document_uri);
                if confirmed_unverified_keys != Some(expected) {
                    return Err(Error::UnverifiedKeyApprovalRequired {
                        did: recipient.did.clone(),
                    });
                }
                Some(expected)
            }
        };

        let now = self.opake.now();

        let content_key = documents::fetch_content_key(
            &mut self.opake.client,
            &cabinet.did,
            &cabinet.private_keys(),
            document_uri,
        )
        .await?;

        sharing::create_grant(
            &mut self.opake.client,
            &GrantParams {
                document_uri,
                recipient_did: &recipient.did,
                content_key: &content_key,
                recipient_public_keys: PublicKeyBundle {
                    x25519: &recipient.x25519_public_key,
                    ml_kem: &recipient.ml_kem_public_key,
                },
                permissions,
                note,
                unverified_key_approval,
                created_at: &now,
            },
            &mut self.opake.rng,
        )
        .await
    }

    /// Revoke a grant (delete the grant record). Cabinet only.
    #[::opake_derive::signoff]
    pub async fn revoke_share(&mut self, grant_uri: &str) -> Result<(), Error> {
        sharing::revoke_grant(&mut self.opake.client, grant_uri).await
    }

    /// List all grants on the caller's PDS.
    #[::opake_derive::signoff]
    pub async fn list_shares(&mut self) -> Result<Vec<GrantEntry>, Error> {
        sharing::list_grants(&mut self.opake.client).await
    }

    /// Enqueue a pending share for a recipient who hasn't set up Opake yet.
    ///
    /// Cabinet only. Fetches the document's content key and writes a
    /// `pendingShare` record encrypted with the grant metadata the daemon
    /// needs to reconstruct the grant once the recipient publishes their
    /// public key.
    #[::opake_derive::signoff]
    pub async fn create_pending_share(
        &mut self,
        document_uri: &str,
        recipient: &str,
        recipient_did: &str,
        allow_unverified_first_publication: bool,
        permissions: &str,
        note: Option<&str>,
    ) -> Result<String, Error> {
        let FileContext::Cabinet(ref cabinet) = self.context else {
            return Err(Error::InvalidRecord(
                "pending shares are only supported from the cabinet".into(),
            ));
        };

        if !allow_unverified_first_publication {
            return Err(Error::UnverifiedKeyApprovalRequired {
                did: recipient_did.to_owned(),
            });
        }

        let now = self.opake.now();

        let content_key = documents::fetch_content_key(
            &mut self.opake.client,
            &cabinet.did,
            &cabinet.private_keys(),
            document_uri,
        )
        .await?;

        sharing::create_pending_share(
            &mut self.opake.client,
            &content_key,
            document_uri,
            recipient,
            recipient_did,
            allow_unverified_first_publication,
            permissions,
            note,
            &now,
            &mut self.opake.rng,
        )
        .await
    }
}

#[cfg(test)]
#[path = "sharing_tests.rs"]
mod tests;
