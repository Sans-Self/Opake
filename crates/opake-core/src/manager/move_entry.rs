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
    /// Cabinet: atomic `applyWrites` — remove from source, add to target.
    ///
    /// Workspace: not yet wired. Move always involves source ≠ target,
    /// and at least one of them must be a non-root subdirectory (you
    /// can't move within the same listing). The deep-cascade builder
    /// (walking root → source and root → target with re-fetches at each
    /// level) is a later slice.
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
            return Err(Error::Unimplemented(
                "workspace move (requires deep cascade — two non-root supersedes)".into(),
            ));
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
