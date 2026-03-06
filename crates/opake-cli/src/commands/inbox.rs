use anyhow::{Context, Result};
use clap::Args;
use opake_core::client::{fetch_inbox_all, InboxGrant, Session};

use crate::commands::Execute;
use crate::identity;
use crate::session::CommandContext;
use opake_core::client::ReqwestTransport;

#[derive(Args)]
/// List grants shared with you (via appview)
pub struct InboxCommand {
    /// Show long format with document URIs and notes
    #[arg(short, long)]
    long: bool,

    /// AppView URL (overrides OPAKE_APPVIEW_URL and config)
    #[arg(long)]
    appview: Option<String>,
}

fn format_short(grants: &[InboxGrant]) -> String {
    grants
        .iter()
        .map(|g| format!("{}\t{}", g.owner_did, g.uri))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_long(grants: &[InboxGrant]) -> String {
    grants
        .iter()
        .map(|g| {
            format!(
                "  {}  {}\n           doc: {}\n           grant: {}",
                g.created_at, g.owner_did, g.document_uri, g.uri,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl Execute for InboxCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let appview_url = ctx.storage.resolve_appview_url(self.appview.as_deref())?;

        let id = identity::load_identity(&ctx.storage, &ctx.did)
            .context("no identity found — run `opake login` first")?;
        let signing_key = id
            .signing_key_bytes()?
            .context("no signing key in identity — re-run `opake login` to migrate")?;

        let transport = ReqwestTransport::new();
        let grants = fetch_inbox_all(&transport, &appview_url, &ctx.did, &signing_key).await?;

        if grants.is_empty() {
            println!("no incoming grants");
            return Ok(None);
        }

        if self.long {
            println!("{}", format_long(&grants));
        } else {
            println!("{}", format_short(&grants));
        }

        println!("\n{} grant(s)", grants.len());

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(owner: &str, doc_suffix: &str) -> InboxGrant {
        InboxGrant {
            uri: "at://did:plc:owner/app.opake.grant/g1".into(),
            owner_did: owner.into(),
            document_uri: format!("at://did:plc:owner/app.opake.document/{doc_suffix}"),
            created_at: "2026-03-01T12:00:00Z".into(),
        }
    }

    #[test]
    fn short_format() {
        let grants = vec![grant("did:plc:alice", "doc1")];
        let output = format_short(&grants);
        assert!(output.contains("did:plc:alice"));
        assert!(output.contains("grant/g1"));
    }

    #[test]
    fn long_format() {
        let grants = vec![grant("did:plc:alice", "doc1")];
        let output = format_long(&grants);
        assert!(output.contains("did:plc:alice"));
        assert!(output.contains("doc: at://"));
        assert!(output.contains("grant: at://"));
    }
}
