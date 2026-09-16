use crate::client::Transport;
use crate::crypto::{CryptoRng, PublicKeyBundle, RngCore};
use crate::documents;
use crate::error::Error;
use crate::resolve::{ResolvedIdentity, VerificationState};
use crate::sharing::{self, GrantEntry, GrantParams};
use crate::storage::Storage;

use super::types::FileContext;
use super::FileManager;

/// An opaque, queue-time recipient resolution for a pending share.
///
/// The entered handle is only display data. The resolved DID may be displayed
/// to the owner, but callers cannot replace the account approved between the
/// warning and the one-use handoff.
#[derive(Debug)]
pub struct PendingShareRecipient {
    recipient: String,
    did: String,
    document_uri: String,
    owner_did: String,
}

/// What was actually written by a direct share. This is derived from the
/// final resolution immediately before wrapping, rather than an earlier UI
/// inspection.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareWriteResult {
    pub uri: String,
    pub recipient_did: String,
    pub verification: VerificationState,
}

impl PendingShareRecipient {
    /// The DID the queue will authorize. Native clients may display this with
    /// their explicit first-publication confirmation.
    pub fn did(&self) -> &str {
        &self.did
    }
}

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
            VerificationState::Unverified => Some(recipient.unverified_key_approval_for_version(
                crate::records::Grant::RECORD_VERSION,
                document_uri,
            )),
        }
    }

    /// Share a document with a freshly resolved recipient DID by creating a grant.
    ///
    /// Cabinet only. Fetches the document's content key, wraps it to the
    /// recipient's hybrid public-key bundle, and creates a grant record.
    ///
    /// Returns the grant URI and verification state observed for the actual
    /// wrapped key.
    #[::opake_derive::signoff]
    pub async fn share(
        &mut self,
        document_uri: &str,
        recipient_did: &str,
        confirmed_unverified_keys: Option<[u8; 32]>,
        permissions: &str,
        note: Option<&str>,
    ) -> Result<ShareWriteResult, Error> {
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
        let unverified_key_approval = match &recipient.verification {
            VerificationState::Verified { .. } => None,
            VerificationState::Unverified => {
                let expected = recipient.unverified_key_approval_for_version(
                    crate::records::Grant::RECORD_VERSION,
                    document_uri,
                );
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

        let uri = sharing::create_grant(
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
                pending_share_uri: None,
                pending_share_commitment: None,
                created_at: &now,
            },
            &mut self.opake.rng,
        )
        .await?;
        Ok(ShareWriteResult {
            uri,
            recipient_did: recipient.did,
            verification: recipient.verification,
        })
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
    /// Resolve the recipient at the same security boundary that will create
    /// the queued intent. The returned challenge must be passed unchanged to
    /// [`Self::create_pending_share`] after explicit first-publication consent.
    pub async fn prepare_pending_share_recipient(
        &self,
        document_uri: &str,
        recipient: &str,
    ) -> Result<PendingShareRecipient, Error> {
        let FileContext::Cabinet(ref cabinet) = self.context else {
            return Err(Error::InvalidRecord(
                "pending shares are only supported from the cabinet".into(),
            ));
        };
        let did = self.opake.resolve_recipient_did(recipient).await?;
        Ok(PendingShareRecipient {
            recipient: recipient.to_owned(),
            did,
            document_uri: document_uri.to_owned(),
            owner_did: cabinet.did.clone(),
        })
    }

    #[::opake_derive::signoff]
    pub async fn create_pending_share(
        &mut self,
        document_uri: &str,
        recipient: &PendingShareRecipient,
        allow_unverified_first_publication: bool,
        permissions: &str,
        note: Option<&str>,
    ) -> Result<String, Error> {
        let FileContext::Cabinet(ref cabinet) = self.context else {
            return Err(Error::InvalidRecord(
                "pending shares are only supported from the cabinet".into(),
            ));
        };

        if recipient.document_uri != document_uri || recipient.owner_did != cabinet.did {
            return Err(Error::InvalidRecord(
                "pending-share recipient challenge belongs to a different document or owner".into(),
            ));
        }

        if !allow_unverified_first_publication {
            return Err(Error::UnverifiedKeyApprovalRequired {
                did: recipient.did.clone(),
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
            &recipient.recipient,
            &recipient.did,
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
