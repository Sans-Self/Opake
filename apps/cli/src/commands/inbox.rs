use anyhow::Result;
use clap::Args;
use opake_core::client::{InboxGrant, Session};

use crate::commands::Execute;
use crate::session::CommandContext;

#[derive(Args)]
/// List grants shared with you (via indexer)
pub struct InboxCommand {
    /// Show long format with document URIs and notes
    #[arg(short, long)]
    long: bool,

    /// Indexer URL (overrides OPAKE_INDEXER_URL and config)
    #[arg(long, value_name = "URL")]
    indexer: Option<String>,
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
        let mut opake = ctx.opake().await?;

        // CLI: flag → env var → account config (core handles the last one).
        let env_url = std::env::var("OPAKE_INDEXER_URL").ok();
        let indexer_url = self.indexer.as_deref().or(env_url.as_deref());

        let grants = opake.list_inbox(indexer_url).await?;

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
