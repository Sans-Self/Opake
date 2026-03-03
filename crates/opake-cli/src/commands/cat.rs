use std::io::Write;

use anyhow::{Context, Result};
use clap::Args;
use opake_core::atproto;
use opake_core::client::Session;
use opake_core::directories::DirectoryTree;
use opake_core::documents;

use crate::commands::Execute;
use crate::identity;
use crate::keyring_store;
use crate::session::{self, CommandContext};

#[derive(Args)]
/// Print a decrypted file to stdout
pub struct CatCommand {
    /// Path, filename, or AT-URI of the document
    reference: String,
}

impl Execute for CatCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let id = identity::load_identity(&ctx.did).context("run `opake login` first")?;
        let private_key = id.private_key_bytes()?;
        let mut client = session::load_client(&ctx.did)?;

        // Resolve the reference to an AT-URI.
        let uri = if self.reference.starts_with("at://") {
            self.reference.clone()
        } else if self.reference.contains('/') {
            // Path — needs the directory tree.
            let tree = DirectoryTree::load(&mut client).await?;
            let resolved = tree.resolve(&mut client, &self.reference).await?;
            resolved.uri
        } else {
            // Bare name — lightweight document resolution.
            documents::resolve_uri(&mut client, &self.reference).await?
        };

        // Peek at the document to check for keyring encryption.
        let at_uri = atproto::parse_at_uri(&uri)?;
        let entry = client
            .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
            .await?;
        let doc: opake_core::records::Document = serde_json::from_value(entry.value)?;

        let group_key = match &doc.encryption {
            opake_core::records::Encryption::Keyring(kr_enc) => {
                let kr_uri = atproto::parse_at_uri(&kr_enc.keyring_ref.keyring)?;
                Some(
                    keyring_store::load_group_key(
                        &ctx.did,
                        &kr_uri.rkey,
                        kr_enc.keyring_ref.rotation,
                    )
                    .context(
                        "if you're a keyring member (not the creator), use: \
                              opake download --keyring-member <document-uri>",
                    )?,
                )
            }
            opake_core::records::Encryption::Direct(_) => None,
        };

        let (_name, plaintext) = documents::download_with_group_key(
            &mut client,
            &id.did,
            &private_key,
            group_key.as_ref(),
            &uri,
        )
        .await?;

        std::io::stdout()
            .write_all(&plaintext)
            .context("failed to write to stdout")?;

        Ok(session::refreshed_session(&client))
    }
}
