use crate::atproto;
use crate::client::{ApplyWriteOp, Transport};
use crate::crypto::{CryptoRng, RngCore};
use crate::directories;
use crate::error::Error;
use crate::records::{DirectoryUpdateRecord, DIRECTORY_UPDATE_COLLECTION};
use crate::storage::Storage;

use super::types::{FileContext, MutationOutcome};
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Move an entry (document or directory) between directories — atomically.
    ///
    /// Owner: source removal + target addition happen in a single `applyWrites`
    /// call. No lost entries on partial failure.
    /// Member: proposes the move via `directoryUpdate`.
    #[::opake_derive::signoff]
    pub async fn move_entry(
        &mut self,
        entry_uri: &str,
        source_dir: &str,
        target_dir: &str,
    ) -> Result<MutationOutcome, Error> {
        if source_dir == target_dir {
            return Err(Error::InvalidRecord(
                "source and target directory are the same".into(),
            ));
        }

        let now = self.opake.now();

        match &self.context {
            FileContext::Cabinet(_) => {
                let remove_op = directories::prepare_remove_entry(
                    &mut self.opake.client,
                    source_dir,
                    entry_uri,
                    &now,
                )
                .await?;
                let add_op = directories::prepare_add_entry(
                    &mut self.opake.client,
                    target_dir,
                    entry_uri,
                    &now,
                )
                .await?;

                // Atomic: remove from source + add to target
                self.opake.client.apply_writes(&[remove_op, add_op]).await?;
                self.invalidate_directory_cache().await;
                Ok(MutationOutcome::Applied)
            }
            FileContext::Workspace(ws) => {
                let dir_owner = atproto::parse_at_uri(source_dir)?.authority;
                if dir_owner == self.opake.did {
                    let remove_op = directories::prepare_remove_entry(
                        &mut self.opake.client,
                        source_dir,
                        entry_uri,
                        &now,
                    )
                    .await?;
                    let add_op = directories::prepare_add_entry(
                        &mut self.opake.client,
                        target_dir,
                        entry_uri,
                        &now,
                    )
                    .await?;

                    // Atomic: remove from source + add to target
                    self.opake.client.apply_writes(&[remove_op, add_op]).await?;
                    self.invalidate_directory_cache().await;
                    Ok(MutationOutcome::Applied)
                } else {
                    let update = DirectoryUpdateRecord::move_entry(
                        ws.uri.clone(),
                        source_dir.to_string(),
                        target_dir.to_string(),
                        entry_uri.to_string(),
                        now,
                    );
                    self.opake
                        .client
                        .apply_writes(&[ApplyWriteOp::Create {
                            collection: DIRECTORY_UPDATE_COLLECTION.into(),
                            rkey: None,
                            record: serde_json::to_value(&update)?,
                        }])
                        .await?;
                    Ok(MutationOutcome::Proposed {
                        update_uri: entry_uri.to_string(),
                    })
                }
            }
        }
    }
}
