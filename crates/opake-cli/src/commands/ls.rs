use anyhow::Result;
use clap::Args;
use opake_core::client::Session;
use opake_core::crypto::DocumentMetadata;

use crate::commands::Execute;
use crate::session::CommandContext;

#[derive(Args)]
/// List files and directories
pub struct LsCommand {
    /// Directory to list (defaults to root "/")
    path: Option<String>,

    /// List a workspace instead of the personal cabinet
    #[arg(long)]
    workspace: Option<String>,

    /// Show long format with sizes and mime types
    #[arg(short, long)]
    long: bool,

    /// Filter by tag
    #[arg(long)]
    tag: Option<String>,
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

impl Execute for LsCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        if self.tag.is_some() && !self.long {
            anyhow::bail!("--tag requires --long format (need metadata to filter)");
        }

        let mut opake = ctx.opake().await?;
        let context = opake.file_context(self.workspace.as_deref()).await?;
        let mut mgr = opake.file_manager(&context);

        let tree = mgr.load_tree().await?;
        let dir = tree.resolve_directory(self.path.as_deref().unwrap_or("/"))?;
        let entries = tree.entries_for(&dir.uri).unwrap_or(&[]);

        // Collect subdirectories (names already decrypted in tree)
        let mut subdirs: Vec<&str> = entries
            .iter()
            .filter_map(|uri| tree.directory_name(uri))
            .collect();
        subdirs.sort_unstable();

        let doc_count;

        if self.long {
            let doc_meta = mgr.resolve_document_metadata_in(&tree, &dir.uri).await?;

            let mut docs: Vec<(&str, &str, &DocumentMetadata)> = doc_meta
                .iter()
                .map(|(uri, meta)| (meta.name.as_str(), uri.as_str(), meta))
                .collect();
            docs.sort_unstable_by_key(|(name, _, _)| *name);

            if let Some(ref tag) = self.tag {
                docs.retain(|(_, _, meta)| meta.tags.iter().any(|t| t == tag));
            }

            doc_count = docs.len();

            if subdirs.is_empty() && docs.is_empty() {
                println!("(empty)");
                return Ok(None);
            }

            for name in &subdirs {
                println!("{name}/");
            }
            for (name, _uri, meta) in &docs {
                let size = meta.size.map(format_size).unwrap_or_else(|| "—".into());
                let mime = meta.mime_type.as_deref().unwrap_or("—");
                let tags = if meta.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", meta.tags.join(", "))
                };
                println!("{:>10}  {}  {}{}", size, mime, name, tags);
            }
        } else {
            let doc_names = mgr.resolve_document_names_in(&tree, &dir.uri).await?;

            let mut docs: Vec<(&str, &str)> = doc_names
                .iter()
                .map(|(uri, name)| (name.as_str(), uri.as_str()))
                .collect();
            docs.sort_unstable_by_key(|(name, _)| *name);

            doc_count = docs.len();

            if subdirs.is_empty() && docs.is_empty() {
                println!("(empty)");
                return Ok(None);
            }

            for name in &subdirs {
                println!("{name}/");
            }
            for (name, _) in &docs {
                println!("{name}");
            }
        }

        println!("\n{} item(s)", subdirs.len() + doc_count);

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
