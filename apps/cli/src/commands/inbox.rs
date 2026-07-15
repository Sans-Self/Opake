use anyhow::Result;
use clap::Args;
use opake_core::atproto;
use opake_core::client::Session;
use opake_core::indexer::IndexerEnvelope;
use opake_core::records::Grant;

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

/// The grant's author DID lives in the at-uri authority position; the
/// indexer no longer hands it back as a flat field. Defaulting to the
/// raw URI keeps the display path infallible if a malformed envelope
/// ever slips through.
fn author_did(envelope: &IndexerEnvelope<Grant>) -> String {
    atproto::parse_at_uri(&envelope.uri)
        .map(|u| u.authority)
        .unwrap_or_else(|_| envelope.uri.clone())
}

fn format_short(grants: &[IndexerEnvelope<Grant>]) -> String {
    grants
        .iter()
        .map(|g| format!("{}\t{}", author_did(g), g.uri))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_long(grants: &[IndexerEnvelope<Grant>]) -> String {
    grants
        .iter()
        .map(|g| {
            format!(
                "  {}  {}\n           doc: {}\n           grant: {}",
                g.record.created_at,
                author_did(g),
                g.record.document,
                g.uri,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl Execute for InboxCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut opake = ctx.opake().await?;

        // `--indexer` promotes to the runtime override (priority 1), above
        // OPAKE_INDEXER_URL (already seeded by `ctx.opake()`) and the user's
        // accountConfig. This is a per-invocation debugging knob.
        if let Some(url) = self.indexer {
            opake.set_indexer_url(url);
        }

        let grants = opake.list_inbox().await?;

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
    use opake_core::atproto::AtBytes;
    use opake_core::records::{EncryptedMetadata, WrappedKey, SCHEMA_VERSION};

    fn grant(author: &str, doc_suffix: &str) -> IndexerEnvelope<Grant> {
        IndexerEnvelope {
            uri: format!("at://{author}/at.opake.grant/g1"),
            record: Grant {
                opake_version: SCHEMA_VERSION,
                document: format!("at://{author}/at.opake.document/{doc_suffix}"),
                recipient: "did:plc:me".into(),
                wrapped_key: WrappedKey {
                    did: "did:plc:me".into(),
                    ciphertext: AtBytes {
                        encoded: String::new(),
                    },
                    algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
                },
                encrypted_metadata: EncryptedMetadata {
                    ciphertext: AtBytes {
                        encoded: String::new(),
                    },
                    nonce: AtBytes {
                        encoded: String::new(),
                    },
                },
                created_at: "2026-03-01T12:00:00Z".into(),
            },
            indexed_at: "2026-03-01T12:00:01Z".into(),
            deleted_at: None,
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
