use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;
use opake_core::client::Session;

use crate::commands::Execute;
use crate::keyring_store;
use crate::session::CommandContext;

/// Download and decrypt a file
///
/// Downloads to a file by default; use --stdout to print to stdout.
/// Also available as `opake cat` (implies --stdout).
#[derive(Args)]
#[command(after_help = "\
Examples:
  opake download secret.pdf
  opake download secret.pdf -o ~/Downloads/
  opake cat secret.pdf
  opake download --grant at://did:plc:abc/at.opake.grant/xyz
  opake download --workspace-member at://did:plc:abc/at.opake.document/xyz")]
pub struct DownloadCommand {
    /// AT URI or filename of the document (not needed with --grant)
    pub reference: Option<String>,

    /// Output path (defaults to the original filename)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Print decrypted content to stdout instead of writing a file
    #[arg(long)]
    pub stdout: bool,

    /// Grant URI for downloading a shared file from another user's PDS
    #[arg(long, conflicts_with = "workspace_member", value_name = "AT-URI")]
    pub grant: Option<String>,

    /// Download a workspace document as a member (cross-PDS, first-time)
    #[arg(long, conflicts_with = "grant", value_name = "AT-URI")]
    pub workspace_member: Option<String>,

    /// Download from a workspace (resolves name within workspace tree)
    #[arg(long)]
    pub workspace: Option<String>,
}

impl Execute for DownloadCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let to_stdout = self.stdout;
        let output_override = self.output.clone();

        // Grant path: cross-PDS download via grant record
        if let Some(grant_uri) = &self.grant {
            let opake = ctx.opake().await?;
            let (name, plaintext) = opake.download_from_grant(grant_uri).await?;
            emit_output(to_stdout, output_override, &name, &plaintext)?;
            return Ok(None);
        }

        // Workspace-member path: cross-PDS first-time download
        if let Some(doc_uri) = &self.workspace_member {
            let mut opake = ctx.opake().await?;
            let result = opake.download_as_workspace_member(doc_uri).await?;

            keyring_store::save_group_key(
                &ctx.storage,
                &ctx.did,
                &result.keyring_rkey,
                result.rotation,
                &result.group_key,
            )?;

            emit_output(
                to_stdout,
                output_override,
                &result.filename,
                &result.plaintext,
            )?;
            return Ok(None);
        }

        // Regular path: resolve by name, download via FileManager
        let reference = self
            .reference
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("provide a document reference or --grant"))?;

        let mut opake = ctx.opake().await?;
        let context = opake.file_context(self.workspace.as_deref()).await?;
        let mut mgr = opake.file_manager(&context);

        let result = mgr.download_at(reference).await?;
        emit_output(
            to_stdout,
            output_override,
            &result.filename,
            &result.plaintext,
        )?;

        Ok(None)
    }
}

fn emit_output(
    to_stdout: bool,
    output_override: Option<PathBuf>,
    filename: &str,
    plaintext: &[u8],
) -> Result<()> {
    if to_stdout {
        std::io::stdout()
            .write_all(plaintext)
            .context("failed to write to stdout")?;
        return Ok(());
    }

    // A user-supplied `-o` path is trusted; a filename derived from decrypted
    // record metadata is not — confine it to a bare name in the current dir.
    let path = match output_override {
        Some(path) => path,
        None => crate::path_safety::record_filename(filename)?,
    };
    write_file(&path, plaintext)?;
    eprintln!(
        "{} → {} ({} bytes)",
        filename,
        path.display(),
        plaintext.len()
    );
    Ok(())
}

fn write_file(path: &Path, content: &[u8]) -> Result<()> {
    if path.exists() {
        anyhow::bail!(
            "output file already exists: {} (use -o to specify a different path)",
            path.display()
        );
    }
    fs::write(path, content).context(format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_file_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("output.txt");
        write_file(&path, b"hello").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hello");
    }

    #[test]
    fn write_file_refuses_to_overwrite() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("existing.txt");
        fs::write(&path, b"original").unwrap();
        let err = write_file(&path, b"new content").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("already exists"), "got: {msg}");
        assert_eq!(fs::read(&path).unwrap(), b"original");
    }

    #[test]
    #[allow(non_snake_case)] // bug__ regression-naming convention
    fn bug__download_rejects_traversal_in_decrypted_filename() {
        // The filename comes from decrypted metadata a malicious workspace
        // member controls; without confinement it escapes the output directory.
        assert_eq!(
            crate::path_safety::record_filename("../../../etc/passwd").unwrap(),
            PathBuf::from("passwd")
        );
        assert!(crate::path_safety::record_filename("/etc/cron.d/evil")
            .unwrap()
            .is_relative());
        assert!(crate::path_safety::record_filename("..").is_err());
    }

    #[test]
    fn emit_stdout_writes_to_stdout() {
        // Just verify it doesn't panic — stdout output is hard to capture in tests
        emit_output(true, None, "test.txt", b"").unwrap();
    }
}
