use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Args;
use opake_core::crypto::OsRng;
use opake_core::documents::{self, UploadParams};

use opake_core::client::Session;

use crate::commands::Execute;
use crate::identity;
use crate::session;

#[derive(Args)]
/// Upload and encrypt a file
pub struct UploadCommand {
    /// Path to the file to encrypt and upload
    path: PathBuf,

    /// Encrypt under a keyring instead of direct keys
    #[arg(long)]
    keyring: Option<String>,

    /// Comma-separated tags for categorization
    #[arg(long, value_delimiter = ',')]
    tags: Vec<String>,
}

impl Execute for UploadCommand {
    async fn execute(self) -> Result<Option<Session>> {
        if self.keyring.is_some() {
            anyhow::bail!("--keyring not yet supported (tracking: chainlink #21)");
        }

        let mut client = session::load_client()?;
        let id = identity::load_identity()?;
        let owner_pubkey = id.public_key_bytes()?;

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

        let params = UploadParams {
            plaintext: &plaintext,
            filename: &filename,
            mime_type,
            owner_did: &id.did,
            owner_pubkey: &owner_pubkey,
            tags: self.tags,
            created_at: &Utc::now().to_rfc3339(),
        };

        let uri = documents::encrypt_and_upload(&mut client, &params, &mut OsRng).await?;

        println!("{} → {}", filename, uri);
        Ok(session::refreshed_session(&client))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_nonexistent_file() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cmd = UploadCommand {
            path: PathBuf::from("/tmp/opake-test-nonexistent-file-abc123"),
            keyring: None,
            tags: vec![],
        };
        let result = rt.block_on(cmd.execute());
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
