use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use opake_core::atproto;
use opake_core::client::Session;
use opake_core::crypto::{ContentKey, OsRng, X25519PrivateKey};
use opake_core::metadata;

use crate::commands::Execute;
use crate::document_resolve;
use crate::identity;
use crate::keyring_store;
use crate::session::{self, CommandContext};

#[derive(Args)]
/// View or modify document metadata (name, tags, description)
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
        let id =
            identity::load_identity(&ctx.storage, &ctx.did).context("run `opake login` first")?;
        let private_key = id.private_key_bytes()?;
        let mut client = session::load_client(&ctx.storage, &ctx.did)?;

        match self.action {
            MetadataAction::Show(args) => {
                let uri = resolve(&mut client, &args.document, &ctx.did, &private_key, ctx).await?;
                let group_key = peek_group_key(&mut client, &uri, ctx).await?;

                let result = metadata::fetch_document_metadata(
                    &mut client,
                    &uri,
                    &ctx.did,
                    &private_key,
                    group_key.as_ref(),
                )
                .await?;

                print_metadata(&result.metadata);
            }
            MetadataAction::Rename(args) => {
                let uri = resolve(&mut client, &args.document, &ctx.did, &private_key, ctx).await?;
                let group_key = peek_group_key(&mut client, &uri, ctx).await?;

                let updated = metadata::update_document_metadata(
                    &mut client,
                    &uri,
                    &ctx.did,
                    &private_key,
                    group_key.as_ref(),
                    &mut OsRng,
                    |m| m.name = args.new_name.clone(),
                )
                .await?;

                println!("Renamed to: {}", updated.name);
            }
            MetadataAction::Describe(args) => {
                let uri = resolve(&mut client, &args.document, &ctx.did, &private_key, ctx).await?;
                let group_key = peek_group_key(&mut client, &uri, ctx).await?;

                if args.clear {
                    metadata::update_document_metadata(
                        &mut client,
                        &uri,
                        &ctx.did,
                        &private_key,
                        group_key.as_ref(),
                        &mut OsRng,
                        |m| m.description = None,
                    )
                    .await?;
                    println!("Description cleared.");
                } else if let Some(text) = args.text {
                    metadata::update_document_metadata(
                        &mut client,
                        &uri,
                        &ctx.did,
                        &private_key,
                        group_key.as_ref(),
                        &mut OsRng,
                        |m| m.description = Some(text.clone()),
                    )
                    .await?;
                    println!("Description updated.");
                } else {
                    anyhow::bail!("provide description text or --clear");
                }
            }
            MetadataAction::Tag(tag_cmd) => match tag_cmd.action {
                TagAction::Add(args) => {
                    let uri =
                        resolve(&mut client, &args.document, &ctx.did, &private_key, ctx).await?;
                    let group_key = peek_group_key(&mut client, &uri, ctx).await?;

                    let updated = metadata::update_document_metadata(
                        &mut client,
                        &uri,
                        &ctx.did,
                        &private_key,
                        group_key.as_ref(),
                        &mut OsRng,
                        |m| {
                            if !m.tags.contains(&args.tag) {
                                m.tags.push(args.tag.clone());
                            }
                        },
                    )
                    .await?;

                    println!(
                        "Tags: {}",
                        if updated.tags.is_empty() {
                            "(none)".into()
                        } else {
                            updated.tags.join(", ")
                        }
                    );
                }
                TagAction::Remove(args) => {
                    let uri =
                        resolve(&mut client, &args.document, &ctx.did, &private_key, ctx).await?;
                    let group_key = peek_group_key(&mut client, &uri, ctx).await?;

                    let updated = metadata::update_document_metadata(
                        &mut client,
                        &uri,
                        &ctx.did,
                        &private_key,
                        group_key.as_ref(),
                        &mut OsRng,
                        |m| m.tags.retain(|t| t != &args.tag),
                    )
                    .await?;

                    println!(
                        "Tags: {}",
                        if updated.tags.is_empty() {
                            "(none)".into()
                        } else {
                            updated.tags.join(", ")
                        }
                    );
                }
            },
        }

        Ok(session::refreshed_session(&client))
    }
}

/// Resolve a document reference to an AT-URI.
async fn resolve(
    client: &mut opake_core::client::XrpcClient<impl opake_core::client::Transport>,
    reference: &str,
    did: &str,
    private_key: &X25519PrivateKey,
    ctx: &CommandContext,
) -> Result<String> {
    let uri =
        document_resolve::resolve_uri(client, reference, did, private_key, &ctx.storage).await?;
    Ok(uri)
}

/// Peek at a document's encryption type and load the group key if keyring-encrypted.
async fn peek_group_key(
    client: &mut opake_core::client::XrpcClient<impl opake_core::client::Transport>,
    uri: &str,
    ctx: &CommandContext,
) -> Result<Option<ContentKey>> {
    let at_uri = atproto::parse_at_uri(uri)?;
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;
    let doc: opake_core::records::Document = serde_json::from_value(entry.value)?;

    match &doc.encryption {
        opake_core::records::Encryption::Keyring(kr_enc) => {
            let kr_uri = atproto::parse_at_uri(&kr_enc.keyring_ref.keyring)?;
            Ok(Some(keyring_store::load_group_key(
                &ctx.storage,
                &ctx.did,
                &kr_uri.rkey,
                kr_enc.keyring_ref.rotation,
            )?))
        }
        opake_core::records::Encryption::Direct(_) => Ok(None),
    }
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
