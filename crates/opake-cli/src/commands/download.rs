use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;
use opake_core::documents;

use opake_core::client::Session;

use crate::commands::Execute;
use crate::identity;
use crate::session::{self, CommandContext};

#[derive(Args)]
/// Download and decrypt a file
pub struct DownloadCommand {
    /// AT URI or filename of the document
    reference: String,

    /// Output path (defaults to the original filename)
    #[arg(short, long)]
    output: Option<PathBuf>,
}

/// Determine where to write the downloaded file. Uses the explicit output path
/// if provided, otherwise falls back to the original filename.
fn resolve_output_path(output_override: Option<PathBuf>, original_name: &str) -> PathBuf {
    output_override.unwrap_or_else(|| PathBuf::from(original_name))
}

/// Write decrypted content to disk, refusing to overwrite existing files.
fn write_output(path: &Path, content: &[u8]) -> Result<()> {
    if path.exists() {
        anyhow::bail!(
            "output file already exists: {} (use -o to specify a different path)",
            path.display()
        );
    }

    fs::write(path, content).context(format!("failed to write {}", path.display()))?;
    Ok(())
}

impl Execute for DownloadCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.did)?;
        let id = identity::load_identity(&ctx.did).context("run `opake login` first")?;
        let private_key = id.private_key_bytes()?;

        let uri = documents::resolve_uri(&mut client, &self.reference).await?;

        let (name, plaintext) =
            documents::download_and_decrypt(&mut client, &id.did, &private_key, &uri).await?;

        let output_path = resolve_output_path(self.output, &name);
        write_output(&output_path, &plaintext)?;

        println!(
            "{} → {} ({} bytes)",
            name,
            output_path.display(),
            plaintext.len()
        );

        Ok(session::refreshed_session(&client))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn resolve_defaults_to_original_filename() {
        let path = resolve_output_path(None, "photo.jpg");
        assert_eq!(path, PathBuf::from("photo.jpg"));
    }

    #[test]
    fn resolve_uses_override_when_provided() {
        let path = resolve_output_path(Some(PathBuf::from("/tmp/custom.bin")), "photo.jpg");
        assert_eq!(path, PathBuf::from("/tmp/custom.bin"));
    }

    #[test]
    fn write_output_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("output.txt");

        write_output(&path, b"hello").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"hello");
    }

    #[test]
    fn write_output_refuses_to_overwrite() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("existing.txt");
        fs::write(&path, b"original").unwrap();

        let err = write_output(&path, b"new content").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("already exists"), "got: {msg}");
        assert!(msg.contains("-o"), "should suggest -o flag, got: {msg}");

        // Original content untouched
        assert_eq!(fs::read(&path).unwrap(), b"original");
    }

    #[test]
    fn write_output_handles_empty_content() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.bin");

        write_output(&path, b"").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"");
    }
}
