use anyhow::Result;
use clap::Args;
use opake_core::documents::{self, DecryptedDocumentEntry};

use opake_core::client::Session;

use crate::commands::Execute;
use crate::document_resolve;
use crate::identity;
use crate::session::{self, CommandContext};

#[derive(Args)]
/// List your documents
pub struct LsCommand {
    /// Filter by tag
    #[arg(long)]
    tag: Option<String>,

    /// Show long format with sizes and dates
    #[arg(short, long)]
    long: bool,
}

/// Format a byte count for humans.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Keep only entries that have the given tag.
fn filter_by_tag(docs: &mut Vec<DecryptedDocumentEntry>, tag: &str) {
    docs.retain(|d| d.metadata.tags.iter().any(|t| t == tag));
}

/// One line per document: name and URI separated by a tab.
fn format_short(docs: &[DecryptedDocumentEntry]) -> String {
    docs.iter()
        .map(|d| format!("{}\t{}", d.metadata.name, d.uri))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Multi-line per document: size, date, mime, name, tags, then URI on the next line.
fn format_long(docs: &[DecryptedDocumentEntry]) -> String {
    docs.iter()
        .map(|d| {
            let size = d
                .metadata
                .size
                .map(format_size)
                .unwrap_or_else(|| "—".into());
            let mime = d.metadata.mime_type.as_deref().unwrap_or("—");
            let tags = if d.metadata.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", d.metadata.tags.join(", "))
            };
            format!(
                "{:>10}  {}  {}  {}{}\n           {}",
                size, d.created_at, mime, d.metadata.name, tags, d.uri,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl Execute for LsCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        let entries = documents::list_documents(&mut client).await?;

        let id = identity::load_identity(&ctx.storage, &ctx.did)?;
        let private_key = id.private_key_bytes()?;

        let mut docs =
            document_resolve::decrypt_entries(&entries, &ctx.did, &private_key, &ctx.storage);

        if let Some(ref tag) = self.tag {
            filter_by_tag(&mut docs, tag);
        }

        if docs.is_empty() {
            if let Some(ref tag) = self.tag {
                println!("no documents matching tag {:?}", tag);
            } else {
                println!("no documents");
            }
            return Ok(session::refreshed_session(&client));
        }

        if self.long {
            println!("{}", format_long(&docs));
        } else {
            println!("{}", format_short(&docs));
        }

        println!("\n{} document(s)", docs.len());

        Ok(session::refreshed_session(&client))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opake_core::crypto::DocumentMetadata;
    use opake_core::records::Encryption;

    fn doc(name: &str, uri: &str, tags: Vec<String>) -> DecryptedDocumentEntry {
        DecryptedDocumentEntry {
            uri: uri.into(),
            created_at: "2026-03-01T00:00:00Z".into(),
            encryption: Encryption::Direct(opake_core::records::DirectEncryption {
                envelope: opake_core::records::EncryptionEnvelope {
                    algo: "aes-256-gcm".into(),
                    nonce: opake_core::records::AtBytes {
                        encoded: "AAAAAAAAAAAAAAAA".into(),
                    },
                    keys: vec![opake_core::records::WrappedKey {
                        did: "did:plc:test".into(),
                        ciphertext: opake_core::records::AtBytes {
                            encoded: "AAAA".into(),
                        },
                        algo: "x25519-hkdf-a256kw".into(),
                    }],
                },
            }),
            metadata: DocumentMetadata {
                name: name.into(),
                mime_type: Some("text/plain".into()),
                size: Some(1024),
                tags,
                description: None,
            },
        }
    }

    // -- format_size --

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
    }

    #[test]
    fn format_size_megabytes() {
        assert_eq!(format_size(1_048_576), "1.0 MB");
        assert_eq!(format_size(5_242_880), "5.0 MB");
    }

    #[test]
    fn format_size_gigabytes() {
        assert_eq!(format_size(1_073_741_824), "1.0 GB");
    }

    // -- filter_by_tag --

    #[test]
    fn filter_keeps_matching_tag() {
        let mut docs = vec![
            doc("a.txt", "at://did/col/a", vec!["photos".into()]),
            doc("b.txt", "at://did/col/b", vec!["docs".into()]),
            doc(
                "c.txt",
                "at://did/col/c",
                vec!["photos".into(), "family".into()],
            ),
        ];
        filter_by_tag(&mut docs, "photos");
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].metadata.name, "a.txt");
        assert_eq!(docs[1].metadata.name, "c.txt");
    }

    #[test]
    fn filter_removes_all_when_no_match() {
        let mut docs = vec![doc("a.txt", "at://did/col/a", vec!["docs".into()])];
        filter_by_tag(&mut docs, "nonexistent");
        assert!(docs.is_empty());
    }

    #[test]
    fn filter_on_empty_list() {
        let mut docs: Vec<DecryptedDocumentEntry> = vec![];
        filter_by_tag(&mut docs, "anything");
        assert!(docs.is_empty());
    }

    // -- format_short --

    #[test]
    fn short_format_single_entry() {
        let docs = vec![doc("notes.txt", "at://did/col/abc", vec![])];
        let output = format_short(&docs);
        assert_eq!(output, "notes.txt\tat://did/col/abc");
    }

    #[test]
    fn short_format_multiple_entries() {
        let docs = vec![
            doc("a.txt", "at://did/col/a", vec![]),
            doc("b.txt", "at://did/col/b", vec![]),
        ];
        let output = format_short(&docs);
        assert_eq!(output, "a.txt\tat://did/col/a\nb.txt\tat://did/col/b");
    }

    // -- format_long --

    #[test]
    fn long_format_includes_size_and_mime() {
        let docs = vec![doc("notes.txt", "at://did/col/abc", vec![])];
        let output = format_long(&docs);
        assert!(
            output.contains("1.0 KB"),
            "should contain formatted size, got: {output}"
        );
        assert!(output.contains("text/plain"), "should contain mime type");
        assert!(output.contains("notes.txt"), "should contain filename");
        assert!(output.contains("at://did/col/abc"), "should contain URI");
    }

    #[test]
    fn long_format_shows_tags() {
        let docs = vec![doc(
            "photo.jpg",
            "at://did/col/p",
            vec!["vacation".into(), "beach".into()],
        )];
        let output = format_long(&docs);
        assert!(output.contains("[vacation, beach]"), "got: {output}");
    }

    #[test]
    fn long_format_no_tags_no_brackets() {
        let docs = vec![doc("doc.pdf", "at://did/col/d", vec![])];
        let output = format_long(&docs);
        assert!(
            !output.contains('['),
            "should not have brackets when no tags"
        );
    }

    #[test]
    fn long_format_missing_size() {
        let mut docs = vec![doc("unknown.bin", "at://did/col/u", vec![])];
        docs[0].metadata.size = None;
        let output = format_long(&docs);
        assert!(
            output.contains('—'),
            "should show dash for missing size, got: {output}"
        );
    }

    #[test]
    fn long_format_missing_mime() {
        let mut docs = vec![doc("mystery", "at://did/col/m", vec![])];
        docs[0].metadata.mime_type = None;
        let output = format_long(&docs);
        assert!(
            output.contains('—'),
            "should show dash for missing mime, got: {output}"
        );
    }
}
