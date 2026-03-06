use anyhow::Result;
use clap::Args;
use opake_core::client::Session;
use opake_core::sharing::{self, GrantEntry};

use crate::commands::Execute;
use crate::session::{self, CommandContext};

#[derive(Args)]
/// List grants you've shared with others
pub struct SharedCommand {
    /// Show long format with document URIs and notes
    #[arg(short, long)]
    long: bool,
}

fn format_short(entries: &[GrantEntry]) -> String {
    entries
        .iter()
        .map(|e| {
            let perms = e.permissions.as_deref().unwrap_or("—");
            format!("{}\t{}\t{}", e.recipient, perms, e.uri)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_long(entries: &[GrantEntry]) -> String {
    entries
        .iter()
        .map(|e| {
            let perms = e.permissions.as_deref().unwrap_or("—");
            let note = e
                .note
                .as_deref()
                .map(|n| format!("\n           note: {n}"))
                .unwrap_or_default();
            format!(
                "{:>10}  {}  {}\n           doc: {}\n           grant: {}{}",
                perms, e.created_at, e.recipient, e.document, e.uri, note,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl Execute for SharedCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        let entries = sharing::list_grants(&mut client).await?;

        if entries.is_empty() {
            println!("no outgoing grants");
            return Ok(session::refreshed_session(&client));
        }

        if self.long {
            println!("{}", format_long(&entries));
        } else {
            println!("{}", format_short(&entries));
        }

        println!("\n{} grant(s)", entries.len());

        Ok(session::refreshed_session(&client))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(recipient: &str, doc: &str, perms: Option<&str>, note: Option<&str>) -> GrantEntry {
        GrantEntry {
            uri: format!("at://did:plc:owner/app.opake.grant/g1"),
            document: doc.into(),
            recipient: recipient.into(),
            permissions: perms.map(|s| s.into()),
            note: note.map(|s| s.into()),
            created_at: "2026-03-01T12:00:00Z".into(),
        }
    }

    #[test]
    fn short_format() {
        let entries = vec![entry(
            "did:plc:bob",
            "at://did:plc:owner/app.opake.document/doc1",
            Some("read"),
            None,
        )];
        let output = format_short(&entries);
        assert!(output.contains("did:plc:bob"));
        assert!(output.contains("read"));
        assert!(output.contains("grant/g1"));
    }

    #[test]
    fn short_format_missing_permissions() {
        let entries = vec![entry("did:plc:bob", "at://doc", None, None)];
        let output = format_short(&entries);
        assert!(output.contains('—'));
    }

    #[test]
    fn long_format_with_note() {
        let entries = vec![entry(
            "did:plc:bob",
            "at://did:plc:owner/app.opake.document/doc1",
            Some("read"),
            Some("tax doc"),
        )];
        let output = format_long(&entries);
        assert!(output.contains("did:plc:bob"));
        assert!(output.contains("doc: at://"));
        assert!(output.contains("grant: at://"));
        assert!(output.contains("note: tax doc"));
    }

    #[test]
    fn long_format_no_note() {
        let entries = vec![entry("did:plc:bob", "at://doc", Some("read"), None)];
        let output = format_long(&entries);
        assert!(!output.contains("note:"));
    }
}
