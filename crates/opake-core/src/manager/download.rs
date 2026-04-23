use crate::atproto;
use crate::client::Transport;
use crate::crypto::{CryptoRng, RngCore};
use crate::directories::EntryKind;
use crate::documents;
use crate::error::Error;
use crate::storage::Storage;

use super::types::{DownloadResult, FileContext};
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Download a document by name or path.
    ///
    /// Loads the tree, resolves the reference to a document URI, then
    /// downloads. This is the high-level counterpart to `download` which
    /// takes a raw URI.
    pub async fn download_at(&mut self, reference: &str) -> Result<DownloadResult, Error> {
        let tree = self.load_tree().await?;
        let resolved = self.resolve_entry(&tree, reference).await?;

        if resolved.kind != EntryKind::Document {
            return Err(Error::NotFound(format!(
                "{:?} is a directory, not a document",
                resolved.name,
            )));
        }

        self.download(&resolved.uri).await
    }

    /// Download and decrypt a document by URI.
    ///
    /// Cabinet: authenticated download from own PDS using private key.
    /// Workspace (owner): authenticated download with group key.
    /// Workspace (member, cross-PDS): unauthenticated public fetch from
    /// the owner's PDS, group key unwrap via keyring record.
    #[::opake_derive::signoff]
    pub async fn download(&mut self, document_uri: &str) -> Result<DownloadResult, Error> {
        match &self.context {
            FileContext::Cabinet(cabinet) => {
                let (filename, plaintext) = documents::download(
                    &mut self.opake.client,
                    &cabinet.did,
                    &cabinet.private_key,
                    document_uri,
                )
                .await?;
                Ok(DownloadResult {
                    filename,
                    plaintext,
                })
            }
            FileContext::Workspace(ws) => {
                let doc_authority = atproto::parse_at_uri(document_uri)?.authority;
                let private_key = self.opake.identity().private_key_bytes()?;

                if doc_authority == self.opake.did {
                    let (filename, plaintext) = documents::download_with_group_key(
                        &mut self.opake.client,
                        &self.opake.did,
                        &private_key,
                        Some(&ws.key),
                        document_uri,
                    )
                    .await?;
                    Ok(DownloadResult {
                        filename,
                        plaintext,
                    })
                } else {
                    let result = documents::download_from_keyring_member(
                        self.opake.client.transport(),
                        &self.opake.did,
                        &private_key,
                        document_uri,
                    )
                    .await?;
                    Ok(DownloadResult {
                        filename: result.filename,
                        plaintext: result.plaintext,
                    })
                }
            }
        }
    }
}
