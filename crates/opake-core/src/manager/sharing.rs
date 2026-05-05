use crate::client::Transport;
use crate::crypto::{CryptoRng, PublicKeyBundle, RngCore};
use crate::documents;
use crate::error::Error;
use crate::sharing::{self, GrantEntry, GrantParams};
use crate::storage::Storage;

use super::types::FileContext;
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Share a document with another user by creating a grant.
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
        recipient_public_keys: PublicKeyBundle<'_>,
        permissions: &str,
        note: Option<&str>,
    ) -> Result<String, Error> {
        let FileContext::Cabinet(ref cabinet) = self.context else {
            return Err(Error::InvalidRecord(
                "sharing is only supported from the cabinet".into(),
            ));
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
                recipient_did,
                content_key: &content_key,
                recipient_public_keys,
                permissions,
                note,
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
        permissions: &str,
        note: Option<&str>,
    ) -> Result<String, Error> {
        let FileContext::Cabinet(ref cabinet) = self.context else {
            return Err(Error::InvalidRecord(
                "pending shares are only supported from the cabinet".into(),
            ));
        };

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
            permissions,
            note,
            &now,
            &mut self.opake.rng,
        )
        .await
    }
}
