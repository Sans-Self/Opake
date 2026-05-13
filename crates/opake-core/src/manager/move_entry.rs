use crate::atproto;
use crate::client::Transport;
use crate::crypto::{CryptoRng, RngCore};
use crate::directories;
use crate::error::Error;
use crate::storage::Storage;

use super::types::{FileContext, MutationOutcome};
use super::FileManager;

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> FileManager<'_, T, R, S> {
    /// Move an entry (document or directory) between directories.
    ///
    /// Source removal + target addition happen in a single `applyWrites`
    /// call so partial failure can't leave the entry registered twice or
    /// nowhere. Today this only succeeds when the source directory lives
    /// on the caller's PDS; the federation rewrite replaces this with a
    /// cascade-driven path any chain participant can author.
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

        if matches!(self.context, FileContext::Workspace(_)) {
            let source_owner = atproto::parse_at_uri(source_dir)?.authority;
            let target_owner = atproto::parse_at_uri(target_dir)?.authority;
            if source_owner != self.opake.did || target_owner != self.opake.did {
                return Err(Error::Unimplemented(
                    "workspace member move (cascade)".into(),
                ));
            }
        }

        // The target directory's listing pins the entry at the CID we just
        // observed — fetch the entry's current CID so add_entry records the
        // version we'll route readers to.
        let parsed = atproto::parse_at_uri(entry_uri)?;
        let entry_record = self
            .opake
            .client
            .get_record(&parsed.authority, &parsed.collection, &parsed.rkey)
            .await?;

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
            &entry_record.cid,
            &now,
        )
        .await?;

        self.opake.client.apply_writes(&[remove_op, add_op]).await?;
        self.invalidate_directory_cache().await;
        Ok(MutationOutcome::Applied)
    }
}
