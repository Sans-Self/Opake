use anyhow::{Context, Result};
use chrono::Utc;
use clap::Args;
use opake_core::client::Session;
use opake_core::crypto::OsRng;
use opake_core::directories::DirectoryTree;
use opake_core::documents;
use opake_core::resolve;
use opake_core::sharing::{self, GrantParams};

use crate::commands::Execute;
use crate::document_resolve;
use crate::identity;
use crate::session::{self, CommandContext};
use opake_core::client::ReqwestTransport;

#[derive(Args)]
/// Share a document with another user
pub struct ShareCommand {
    /// AT URI or filename of the document
    document: String,

    /// Handle or DID of the recipient
    recipient: String,

    /// Optional note to the recipient
    #[arg(short, long)]
    note: Option<String>,
}

impl Execute for ShareCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        let id =
            identity::load_identity(&ctx.storage, &ctx.did).context("run `opake login` first")?;
        let private_key = id.private_key_bytes()?;

        let mut tree = DirectoryTree::load(&mut client).await?;
        tree.decrypt_names(&ctx.did, &private_key);
        let mut resolver = document_resolve::CliDocumentNameResolver::new(
            &mut client,
            &ctx.did,
            &private_key,
            &ctx.storage,
        );
        let resolved = tree.resolve(&mut resolver, &self.document).await?;
        let uri = resolved.uri;

        let content_key =
            documents::fetch_content_key(&mut client, &id.did, &private_key, &uri).await?;

        let transport = ReqwestTransport::new();
        let recipient =
            resolve::resolve_identity(&transport, &ctx.pds_url, &self.recipient).await?;

        let now = Utc::now().to_rfc3339();
        let params = GrantParams {
            document_uri: &uri,
            recipient_did: &recipient.did,
            content_key: &content_key,
            recipient_public_key: &recipient.public_key,
            permissions: "read",
            note: self.note.as_deref(),
            created_at: &now,
        };

        let grant_uri = sharing::create_grant(&mut client, &params, &mut OsRng).await?;

        let display_recipient = recipient.handle.as_deref().unwrap_or(&recipient.did);
        println!("shared with {} → {}", display_recipient, grant_uri);

        Ok(session::refreshed_session(&client))
    }
}
