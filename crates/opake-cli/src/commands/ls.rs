use anyhow::Result;
use clap::Args;
use opake_core::documents::{self, DocumentEntry};

use opake_core::client::Session;

use crate::commands::Execute;
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
fn filter_by_tag(entries: &mut Vec<DocumentEntry>, tag: &str) {
    entries.retain(|e| e.tags.iter().any(|t| t == tag));
}

/// One line per document: name and URI separated by a tab.
fn format_short(entries: &[DocumentEntry]) -> String {
    entries
        .iter()
        .map(|e| format!("{}\t{}", e.name, e.uri))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Multi-line per document: size, date, mime, name, tags, then URI on the next line.
fn format_long(entries: &[DocumentEntry]) -> String {
    entries
        .iter()
        .map(|e| {
            let size = e.size.map(format_size).unwrap_or_else(|| "—".into());
            let mime = e.mime_type.as_deref().unwrap_or("—");
            let tags = if e.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", e.tags.join(", "))
            };
            format!(
                "{:>10}  {}  {}  {}{}\n           {}",
                size, e.created_at, mime, e.name, tags, e.uri,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl Execute for LsCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;
        let mut entries = documents::list_documents(&mut client).await?;

        if let Some(ref tag) = self.tag {
            filter_by_tag(&mut entries, tag);
        }

        if entries.is_empty() {
            if let Some(ref tag) = self.tag {
                println!("no documents matching tag {:?}", tag);
            } else {
                println!("no documents");
            }
            return Ok(session::refreshed_session(&client));
        }

        if self.long {
            println!("{}", format_long(&entries));
        } else {
            println!("{}", format_short(&entries));
        }

        println!("\n{} document(s)", entries.len());

        Ok(session::refreshed_session(&client))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, uri: &str, tags: Vec<String>) -> DocumentEntry {
        DocumentEntry {
            uri: uri.into(),
            name: name.into(),
            size: Some(1024),
            mime_type: Some("text/plain".into()),
            tags,
            created_at: "2026-03-01T00:00:00Z".into(),
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
        let mut entries = vec![
            entry("a.txt", "at://did/col/a", vec!["photos".into()]),
            entry("b.txt", "at://did/col/b", vec!["docs".into()]),
            entry(
                "c.txt",
                "at://did/col/c",
                vec!["photos".into(), "family".into()],
            ),
        ];
        filter_by_tag(&mut entries, "photos");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a.txt");
        assert_eq!(entries[1].name, "c.txt");
    }

    #[test]
    fn filter_removes_all_when_no_match() {
        let mut entries = vec![entry("a.txt", "at://did/col/a", vec!["docs".into()])];
        filter_by_tag(&mut entries, "nonexistent");
        assert!(entries.is_empty());
    }

    #[test]
    fn filter_on_empty_list() {
        let mut entries: Vec<DocumentEntry> = vec![];
        filter_by_tag(&mut entries, "anything");
        assert!(entries.is_empty());
    }

    // -- format_short --

    #[test]
    fn short_format_single_entry() {
        let entries = vec![entry("notes.txt", "at://did/col/abc", vec![])];
        let output = format_short(&entries);
        assert_eq!(output, "notes.txt\tat://did/col/abc");
    }

    #[test]
    fn short_format_multiple_entries() {
        let entries = vec![
            entry("a.txt", "at://did/col/a", vec![]),
            entry("b.txt", "at://did/col/b", vec![]),
        ];
        let output = format_short(&entries);
        assert_eq!(output, "a.txt\tat://did/col/a\nb.txt\tat://did/col/b");
    }

    // -- format_long --

    #[test]
    fn long_format_includes_size_and_mime() {
        let entries = vec![entry("notes.txt", "at://did/col/abc", vec![])];
        let output = format_long(&entries);
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
        let entries = vec![entry(
            "photo.jpg",
            "at://did/col/p",
            vec!["vacation".into(), "beach".into()],
        )];
        let output = format_long(&entries);
        assert!(output.contains("[vacation, beach]"), "got: {output}");
    }

    #[test]
    fn long_format_no_tags_no_brackets() {
        let entries = vec![entry("doc.pdf", "at://did/col/d", vec![])];
        let output = format_long(&entries);
        assert!(
            !output.contains('['),
            "should not have brackets when no tags"
        );
    }

    #[test]
    fn long_format_missing_size() {
        let mut entries = vec![entry("unknown.bin", "at://did/col/u", vec![])];
        entries[0].size = None;
        let output = format_long(&entries);
        assert!(
            output.contains('—'),
            "should show dash for missing size, got: {output}"
        );
    }

    #[test]
    fn long_format_missing_mime() {
        let mut entries = vec![entry("mystery", "at://did/col/m", vec![])];
        entries[0].mime_type = None;
        let output = format_long(&entries);
        assert!(
            output.contains('—'),
            "should show dash for missing mime, got: {output}"
        );
    }
}
