use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;
use opake_core::atproto;
use opake_core::documents;

use opake_core::client::Session;

use crate::commands::Execute;
use crate::identity;
use crate::keyring_store;
use crate::session::{self, CommandContext};
use crate::transport::ReqwestTransport;

#[derive(Args)]
/// Download and decrypt a file
pub struct DownloadCommand {
    /// AT URI or filename of the document (not needed with --grant)
    reference: Option<String>,

    /// Output path (defaults to the original filename)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Grant URI for downloading a shared file from another user's PDS
    #[arg(long, conflicts_with = "keyring_member")]
    grant: Option<String>,

    /// Download a keyring-encrypted document as a member (cross-PDS)
    #[arg(long, conflicts_with = "grant")]
    keyring_member: Option<String>,
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
        let id = identity::load_identity(&ctx.did).context("run `opake login` first")?;
        let private_key = id.private_key_bytes()?;

        let (name, plaintext, refreshed) = if let Some(grant_uri) = &self.grant {
            // Cross-PDS shared download via grant
            let transport = ReqwestTransport::new();
            let (name, plaintext) =
                documents::download_from_grant(&transport, &private_key, grant_uri).await?;
            (name, plaintext, None)
        } else if let Some(doc_uri) = &self.keyring_member {
            // Cross-PDS keyring member download
            let transport = ReqwestTransport::new();
            let result =
                documents::download_from_keyring_member(&transport, &id.did, &private_key, doc_uri)
                    .await?;

            // Cache the group key so subsequent downloads use the local path
            let kr_rkey = &result.keyring_rkey;
            keyring_store::save_group_key(&ctx.did, kr_rkey, &result.group_key)?;

            (result.filename, result.plaintext, None)
        } else {
            // Own-PDS download: use authenticated client
            let reference = self
                .reference
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("provide a document reference or --grant"))?;
            let mut client = session::load_client(&ctx.did)?;
            let uri = documents::resolve_uri(&mut client, reference).await?;

            // Peek at the document to check if it uses keyring encryption.
            // If so, load the local group key before attempting decryption.
            let at_uri = atproto::parse_at_uri(&uri)?;
            let entry = client
                .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
                .await?;
            let doc: opake_core::records::Document = serde_json::from_value(entry.value)?;

            let group_key = match &doc.encryption {
                opake_core::records::Encryption::Keyring(kr_enc) => {
                    let kr_uri = atproto::parse_at_uri(&kr_enc.keyring_ref.keyring)?;
                    Some(
                        keyring_store::load_group_key(&ctx.did, &kr_uri.rkey).context(
                            "if you're a keyring member (not the creator), use: \
                                  opake download --keyring-member <document-uri>",
                        )?,
                    )
                }
                opake_core::records::Encryption::Direct(_) => None,
            };

            let (name, plaintext) = documents::download_with_group_key(
                &mut client,
                &id.did,
                &private_key,
                group_key.as_ref(),
                &uri,
            )
            .await?;
            (name, plaintext, session::refreshed_session(&client))
        };

        let output_path = resolve_output_path(self.output, &name);
        write_output(&output_path, &plaintext)?;

        println!(
            "{} → {} ({} bytes)",
            name,
            output_path.display(),
            plaintext.len()
        );

        Ok(refreshed)
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
