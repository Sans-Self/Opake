use anyhow::Result;
use clap::{Args, Subcommand};
use opake_core::client::Session;

use crate::commands::Execute;
use crate::session::CommandContext;

/// View or modify document metadata (name, tags, description)
///
/// All metadata is encrypted client-side. Record-level fields on the PDS
/// contain only dummy values.
#[derive(Args)]
#[command(after_help = "\
Examples:
  opake metadata show report.pdf
  opake metadata rename report.pdf quarterly-report.pdf
  opake metadata describe report.pdf \"Q4 financial summary\"
  opake metadata tag add report.pdf finance
  opake metadata tag remove report.pdf draft")]
pub struct MetadataCommand {
    #[command(subcommand)]
    action: MetadataAction,
}

#[derive(Subcommand)]
enum MetadataAction {
    /// Display a document's metadata
    Show(ShowArgs),
    /// Rename a document
    Rename(RenameArgs),
    /// Set or clear a document's description
    Describe(DescribeArgs),
    /// Add or remove tags
    Tag(TagCommand),
}

#[derive(Args)]
struct ShowArgs {
    /// Document name, path, or AT-URI
    document: String,
}

#[derive(Args)]
struct RenameArgs {
    /// Document name, path, or AT-URI
    document: String,
    /// New name for the document
    new_name: String,
}

#[derive(Args)]
struct DescribeArgs {
    /// Document name, path, or AT-URI
    document: String,
    /// New description text (omit with --clear to remove)
    text: Option<String>,
    /// Clear the description
    #[arg(long, conflicts_with = "text")]
    clear: bool,
}

#[derive(Args)]
struct TagCommand {
    #[command(subcommand)]
    action: TagAction,
}

#[derive(Subcommand)]
enum TagAction {
    /// Add a tag to a document
    Add(TagArgs),
    /// Remove a tag from a document
    Remove(TagArgs),
}

#[derive(Args)]
struct TagArgs {
    /// Document name, path, or AT-URI
    document: String,
    /// Tag to add or remove
    tag: String,
}

impl Execute for MetadataCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut opake = ctx.opake().await?;
        let context = opake.cabinet_context()?;
        let mut mgr = opake.file_manager(&context);

        match self.action {
            MetadataAction::Show(args) => {
                let tree = mgr.load_tree().await?;
                let resolved = mgr.resolve_entry(&tree, &args.document).await?;

                let metadata = mgr.read_metadata(&resolved.uri).await?;

                print_metadata(&metadata);
            }
            MetadataAction::Rename(args) => {
                let tree = mgr.load_tree().await?;
                let resolved = mgr.resolve_entry(&tree, &args.document).await?;

                let updated = mgr
                    .update_metadata(&resolved.uri, |m| m.name = args.new_name.clone())
                    .await?;

                println!("Renamed to: {}", updated.name);
            }
            MetadataAction::Describe(args) => {
                let tree = mgr.load_tree().await?;
                let resolved = mgr.resolve_entry(&tree, &args.document).await?;

                if args.clear {
                    mgr.update_metadata(&resolved.uri, |m| m.description = None)
                        .await?;
                    println!("Description cleared.");
                } else if let Some(text) = args.text {
                    mgr.update_metadata(&resolved.uri, |m| {
                        m.description = Some(text.clone());
                    })
                    .await?;
                    println!("Description updated.");
                } else {
                    anyhow::bail!("provide description text or --clear");
                }
            }
            MetadataAction::Tag(tag_cmd) => match tag_cmd.action {
                TagAction::Add(args) => {
                    let tree = mgr.load_tree().await?;
                    let resolved = mgr.resolve_entry(&tree, &args.document).await?;

                    let updated = mgr
                        .update_metadata(&resolved.uri, |m| {
                            if !m.tags.contains(&args.tag) {
                                m.tags.push(args.tag.clone());
                            }
                        })
                        .await?;

                    print_tags(&updated.tags);
                }
                TagAction::Remove(args) => {
                    let tree = mgr.load_tree().await?;
                    let resolved = mgr.resolve_entry(&tree, &args.document).await?;

                    let updated = mgr
                        .update_metadata(&resolved.uri, |m| {
                            m.tags.retain(|t| t != &args.tag);
                        })
                        .await?;

                    print_tags(&updated.tags);
                }
            },
        }

        Ok(None)
    }
}

fn print_tags(tags: &[String]) {
    println!(
        "Tags: {}",
        if tags.is_empty() {
            "(none)".into()
        } else {
            tags.join(", ")
        }
    );
}

fn print_metadata(metadata: &opake_core::crypto::DocumentMetadata) {
    println!("Name:        {}", metadata.name);
    if let Some(mime) = &metadata.mime_type {
        println!("MIME type:   {mime}");
    }
    if let Some(size) = metadata.size {
        println!("Size:        {size} bytes");
    }
    if !metadata.tags.is_empty() {
        println!("Tags:        {}", metadata.tags.join(", "));
    }
    if let Some(desc) = &metadata.description {
        println!("Description: {desc}");
    }
}
