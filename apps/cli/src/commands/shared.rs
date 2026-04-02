use anyhow::Result;
use clap::Args;
use opake_core::client::Session;
use opake_core::sharing::GrantEntry;

use crate::commands::Execute;
use crate::session::CommandContext;

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
        .map(|e| format!("{}\t{}", e.recipient, e.uri))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_long(entries: &[GrantEntry]) -> String {
    entries
        .iter()
        .map(|e| {
            format!(
                "  {}  {}\n           doc: {}\n           grant: {}",
                e.created_at, e.recipient, e.document, e.uri,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl Execute for SharedCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut opake = ctx.opake().await?;
        let context = opake.cabinet_context()?;
        let mut mgr = opake.file_manager(&context);

        let entries = mgr.list_shares().await?;

        if entries.is_empty() {
            println!("no outgoing grants");
            return Ok(None);
        }

        if self.long {
            println!("{}", format_long(&entries));
        } else {
            println!("{}", format_short(&entries));
        }

        println!("\n{} grant(s)", entries.len());

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_encrypted_metadata() -> opake_core::records::EncryptedMetadata {
        opake_core::records::EncryptedMetadata {
            ciphertext: opake_core::records::AtBytes {
                encoded: "AAAA".into(),
            },
            nonce: opake_core::records::AtBytes {
                encoded: "BBBB".into(),
            },
        }
    }

    fn entry(recipient: &str, doc: &str) -> GrantEntry {
        GrantEntry {
            uri: "at://did:plc:owner/app.opake.grant/g1".to_string(),
            document: doc.into(),
            recipient: recipient.into(),
            encrypted_metadata: dummy_encrypted_metadata(),
            expires_at: None,
            created_at: "2026-03-01T12:00:00Z".into(),
        }
    }

    #[test]
    fn short_format() {
        let entries = vec![entry(
            "did:plc:bob",
            "at://did:plc:owner/app.opake.document/doc1",
        )];
        let output = format_short(&entries);
        assert!(output.contains("did:plc:bob"));
        assert!(output.contains("grant/g1"));
    }

    #[test]
    fn long_format() {
        let entries = vec![entry(
            "did:plc:bob",
            "at://did:plc:owner/app.opake.document/doc1",
        )];
        let output = format_long(&entries);
        assert!(output.contains("did:plc:bob"));
        assert!(output.contains("doc: at://"));
        assert!(output.contains("grant: at://"));
    }
}
