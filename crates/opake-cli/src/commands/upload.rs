use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Utc;
use clap::Args;
use log::debug;
use opake_core::crypto::{self, OsRng};
use opake_core::records::{AtBytes, DirectEncryption, Document, Encryption, EncryptionEnvelope};

use crate::commands::Execute;
use crate::identity;
use crate::session;

// 50MB, Bluesky PDS default (I think). We might want to make this dynamic at some point.
const MAX_BLOB_SIZE: u64 = 50 * 1024 * 1024;
const DOCUMENT_COLLECTION: &str = "app.opake.cloud.document";

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
    async fn execute(self) -> Result<()> {
        if self.keyring.is_some() {
            anyhow::bail!("--keyring not yet supported (tracking: chainlink #21)");
        }

        let client = session::load_client()?;
        let id = identity::load_identity()?;
        let owner_pubkey = id.public_key_bytes()?;

        let plaintext =
            fs::read(&self.path).context(format!("failed to read {}", self.path.display()))?;

        let file_size = plaintext.len() as u64;
        anyhow::ensure!(
            file_size <= MAX_BLOB_SIZE,
            "file is {} bytes — PDS blob limit is {} bytes (50 MB)",
            file_size,
            MAX_BLOB_SIZE
        );

        let filename = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".into());

        let mime_type = mime_guess::from_path(&self.path)
            .first_raw()
            .unwrap_or("application/octet-stream");

        debug!(
            "encrypting {} ({} bytes, {})",
            filename, file_size, mime_type
        );

        let rng = &mut OsRng;
        let content_key = crypto::generate_content_key(rng);
        let payload = crypto::encrypt_blob(&content_key, &plaintext, rng)?;

        debug!(
            "uploading encrypted blob ({} bytes)",
            payload.ciphertext.len()
        );

        let blob_ref = client
            .upload_blob(payload.ciphertext, "application/octet-stream")
            .await?;

        let wrapped_key = crypto::wrap_key(&content_key, &owner_pubkey, &id.did, rng)?;

        let document = Document {
            mime_type: Some(mime_type.into()),
            size: Some(file_size),
            tags: self.tags,
            visibility: Some("private".into()),
            ..Document::new(
                filename.clone(),
                blob_ref,
                Encryption::Direct(DirectEncryption {
                    envelope: EncryptionEnvelope {
                        algo: "aes-256-gcm".into(),
                        nonce: AtBytes {
                            encoded: BASE64.encode(payload.nonce),
                        },
                        keys: vec![wrapped_key],
                    },
                }),
                Utc::now().to_rfc3339(),
            )
        };

        let record_ref = client.create_record(DOCUMENT_COLLECTION, &document).await?;

        println!("{} → {}", filename, record_ref.uri);
        Ok(())
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
        // Fails at either session loading or file reading depending on env
        assert!(
            err.contains("failed to read") || err.contains("run `opake login` first"),
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
