use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use opake_core::client::Session;

use crate::commands::Execute;
use crate::session::CommandContext;

/// Upload and encrypt a file
///
/// Files are encrypted client-side with AES-256-GCM before upload.
/// MIME type is auto-detected from the file extension.
#[derive(Args)]
#[command(after_help = "\
Examples:
  opake upload photo.jpg
  opake upload doc.pdf --dir projects/
  opake upload data.csv --workspace team
  opake upload notes.md --description \"meeting notes\"")]
pub struct UploadCommand {
    /// Path to the file to encrypt and upload
    path: PathBuf,

    /// Encrypt under a workspace (shared keyring)
    #[arg(long)]
    workspace: Option<String>,

    /// Optional description for the document
    #[arg(long)]
    description: Option<String>,

    /// Directory to place the document in (defaults to root "/")
    #[arg(long)]
    dir: Option<String>,
}

impl Execute for UploadCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
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

        let mut opake = ctx.opake().await?;
        let context = opake.file_context(self.workspace.as_deref()).await?;
        let mut mgr = opake.file_manager(&context);

        let result = mgr
            .upload_at(
                &plaintext,
                &filename,
                mime_type,
                self.description.as_deref(),
                self.dir.as_deref(),
            )
            .await?;

        let dir_label = self.dir.as_deref().unwrap_or("/");
        println!("{} → {} (in {})", filename, result.uri, dir_label);

        Ok(None)
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
            workspace: None,
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
                || err.contains("log in first")
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
