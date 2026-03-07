use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Args;
use opake_core::atproto;
use opake_core::crypto::OsRng;
use opake_core::directories::{self, DirectoryTree, EntryKind};
use opake_core::documents::{self, KeyringUploadParams, UploadParams};
use opake_core::keyrings;

use opake_core::client::Session;

use crate::commands::Execute;
use crate::identity;
use crate::keyring_store;
use crate::session::{self, CommandContext};

#[derive(Args)]
/// Upload and encrypt a file
pub struct UploadCommand {
    /// Path to the file to encrypt and upload
    path: PathBuf,

    /// Encrypt under a keyring instead of direct keys
    #[arg(long)]
    keyring: Option<String>,

    /// Optional description for the document
    #[arg(long)]
    description: Option<String>,

    /// Place the uploaded document into a directory
    #[arg(long)]
    dir: Option<String>,
}

impl Execute for UploadCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;

        let plaintext =
            fs::read(&self.path).context(format!("failed to read {}", self.path.display()))?;

        let filename = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".into());

        let mime_type = mime_guess::from_path(&self.path)
            .first_raw()
            .unwrap_or("application/octet-stream");

        let now = Utc::now().to_rfc3339();

        let uri = if let Some(keyring_name) = &self.keyring {
            let id = identity::load_identity(&ctx.storage, &ctx.did)?;
            let private_key = id.private_key_bytes()?;
            let entry =
                keyrings::resolve_keyring_uri(&mut client, keyring_name, &id.did, &private_key)
                    .await?;
            let at_uri = atproto::parse_at_uri(&entry.uri)?;
            let group_key = keyring_store::load_group_key(
                &ctx.storage,
                &ctx.did,
                &at_uri.rkey,
                entry.rotation,
            )?;

            let params = KeyringUploadParams {
                plaintext: &plaintext,
                filename: &filename,
                mime_type,
                keyring_uri: &entry.uri,
                group_key: &group_key,
                rotation: entry.rotation,
                description: self.description.as_deref(),
                created_at: &now,
            };

            documents::encrypt_and_upload_keyring(&mut client, &params, &mut OsRng).await?
        } else {
            let id = identity::load_identity(&ctx.storage, &ctx.did)?;
            let owner_pubkey = id.public_key_bytes()?;

            let params = UploadParams {
                plaintext: &plaintext,
                filename: &filename,
                mime_type,
                owner_did: &id.did,
                owner_pubkey: &owner_pubkey,
                description: self.description.as_deref(),
                created_at: &now,
            };

            documents::encrypt_and_upload(&mut client, &params, &mut OsRng).await?
        };

        if let Some(dir_path) = &self.dir {
            let id = identity::load_identity(&ctx.storage, &ctx.did)?;
            let private_key = id.private_key_bytes()?;
            let mut tree = DirectoryTree::load(&mut client).await?;
            tree.decrypt_names(&ctx.did, &private_key);
            let resolved = tree.resolve(&mut client, dir_path).await?;

            if resolved.kind != EntryKind::Directory {
                anyhow::bail!("{dir_path:?} is not a directory");
            }

            directories::add_entry(&mut client, &resolved.uri, &uri, &now).await?;
            println!("{} → {} (in {})", filename, uri, dir_path);
        } else {
            println!("{} → {}", filename, uri);
        }

        Ok(session::refreshed_session(&client))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_harness::test_storage;

    #[test]
    fn rejects_nonexistent_file() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (_dir, storage) = test_storage();
        let cmd = UploadCommand {
            path: PathBuf::from("/tmp/opake-test-nonexistent-file-abc123"),
            keyring: None,
            description: None,
            dir: None,
        };
        let ctx = CommandContext {
            did: "did:plc:test".into(),
            pds_url: "https://pds.test".into(),
            storage: storage.clone(),
        };
        let result = rt.block_on(cmd.execute(&ctx));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("failed to read")
                || err.contains("run `opake login` first")
                || err.contains("config.toml"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn mime_detection_works() {
        assert_eq!(
            mime_guess::from_path("photo.jpg").first_raw(),
            Some("image/jpeg")
        );
        assert_eq!(
            mime_guess::from_path("doc.pdf").first_raw(),
            Some("application/pdf")
        );
        assert_eq!(mime_guess::from_path("mystery").first_raw(), None);
    }
}
