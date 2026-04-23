use anyhow::Result;
use clap::{Args, Subcommand};
use opake_core::atproto;
use opake_core::client::{ReqwestTransport, Session};
use opake_core::crypto::DidMember;
use opake_core::keyrings;
use opake_core::records::Role;
use opake_core::resolve;

use crate::commands::Execute;
use crate::keyring_store;
use crate::session::CommandContext;

/// Manage workspaces (shared encrypted file spaces)
///
/// Workspaces let multiple users share encrypted files. Members share
/// a workspace key; documents encrypted to the workspace are accessible
/// to all members.
#[derive(Args)]
#[command(after_help = "\
Examples:
  opake workspace create family-photos
  opake workspace add-member family-photos bob.bsky.social
  opake workspace ls -l
  opake workspace leave family-photos
  opake workspace remove-member family-photos bob.bsky.social")]
pub struct WorkspaceCommand {
    #[command(subcommand)]
    action: WorkspaceAction,
}

#[derive(Subcommand)]
enum WorkspaceAction {
    /// Create a new workspace
    Create(CreateArgs),
    /// List workspaces
    Ls(LsArgs),
    /// Add a member to a workspace
    AddMember(AddMemberArgs),
    /// Leave a workspace
    Leave(LeaveArgs),
    /// Remove a member from a workspace (rotates workspace key)
    RemoveMember(RemoveMemberArgs),
}

#[derive(Args)]
struct CreateArgs {
    /// Name for the workspace
    name: String,
}

#[derive(Args)]
struct LsArgs {
    /// Show long format with URIs and rotation counts
    #[arg(short, long)]
    long: bool,
}

#[derive(Args)]
struct AddMemberArgs {
    /// Workspace name
    workspace: String,
    /// Handle or DID of the new member
    member: String,
    /// Role for the new member (manager, editor, viewer)
    #[arg(short, long, default_value = "editor")]
    role: Role,
}

#[derive(Args)]
struct LeaveArgs {
    /// Workspace name
    workspace: String,
    /// Skip confirmation prompt
    #[arg(short, long)]
    yes: bool,
}

#[derive(Args)]
struct RemoveMemberArgs {
    /// Workspace name
    workspace: String,
    /// Handle or DID of the member to remove
    member: String,
    /// Skip confirmation prompt
    #[arg(short, long)]
    yes: bool,
}

impl Execute for WorkspaceCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        match self.action {
            WorkspaceAction::Create(args) => create(ctx, args).await,
            WorkspaceAction::Ls(args) => ls(ctx, args).await,
            WorkspaceAction::AddMember(args) => add_member(ctx, args).await,
            WorkspaceAction::Leave(args) => leave(ctx, args).await,
            WorkspaceAction::RemoveMember(args) => remove_member(ctx, args).await,
        }
    }
}

async fn create(ctx: &CommandContext, args: CreateArgs) -> Result<Option<Session>> {
    let mut opake = ctx.opake().await?;
    let (uri, group_key) = opake.create_workspace(&args.name, None).await?;

    let at_uri = atproto::parse_at_uri(&uri)?;
    keyring_store::save_group_key(&ctx.storage, &ctx.did, &at_uri.rkey, 0, &group_key)?;

    println!("{} → {}", args.name, uri);
    Ok(None)
}

async fn ls(ctx: &CommandContext, args: LsArgs) -> Result<Option<Session>> {
    let mut opake = ctx.opake().await?;

    // Single source: the indexer sees every keyring the caller is a
    // member of — owned workspaces included, since the owner is always a
    // member of their own. Staleness window exists after `workspace
    // create` until Jetstream delivers the commit to the indexer's firehose consumer.
    let keyrings = opake.discover_member_keyrings().await?;

    if keyrings.is_empty() {
        println!("no workspaces");
        return Ok(None);
    }

    let private_key = opake.require_identity()?.private_key_bytes()?;
    let did = opake.did();

    for kr in &keyrings {
        let name = keyrings::decrypt_indexer_keyring_name(kr, did, &private_key)
            .unwrap_or_else(|| "<encrypted>".into());
        let role_tag = if kr.owner_did == did {
            ""
        } else {
            "\t(member)"
        };

        if args.long {
            println!(
                "{}\t{} member(s)\trotation:{}\t{}{}",
                name,
                kr.members.len(),
                kr.rotation,
                kr.uri,
                role_tag,
            );
        } else {
            println!("{}\t{} member(s){}", name, kr.members.len(), role_tag);
        }
    }

    println!("\n{} workspace(s)", keyrings.len());
    Ok(None)
}

async fn add_member(ctx: &CommandContext, args: AddMemberArgs) -> Result<Option<Session>> {
    let mut opake = ctx.opake().await?;
    let workspace = opake.resolve_workspace(&args.workspace).await?;

    let transport = ReqwestTransport::new();
    let resolved = resolve::resolve_identity(&transport, &ctx.pds_url, &args.member).await?;

    opake
        .add_workspace_member(
            &workspace.uri,
            &workspace.key,
            &resolved.did,
            &resolved.public_key,
            args.role,
        )
        .await?;

    let display = resolved.handle.as_deref().unwrap_or(&resolved.did);
    println!("added {} to {} ({})", display, args.workspace, args.role);
    Ok(None)
}

async fn leave(ctx: &CommandContext, args: LeaveArgs) -> Result<Option<Session>> {
    let mut opake = ctx.opake().await?;
    let workspace = opake.resolve_workspace(&args.workspace).await?;

    if !args.yes && !crate::prompt::confirm(&format!("leave workspace {:?}?", args.workspace))? {
        println!("aborted");
        return Ok(None);
    }

    opake.leave_workspace(&workspace.uri).await?;
    println!("left {}", args.workspace);
    Ok(None)
}

async fn remove_member(ctx: &CommandContext, args: RemoveMemberArgs) -> Result<Option<Session>> {
    let mut opake = ctx.opake().await?;
    let workspace = opake.resolve_workspace(&args.workspace).await?;
    let at_uri = atproto::parse_at_uri(&workspace.uri)?;

    let resolved = opake.resolve_identity(&args.member).await?;
    let display = resolved.handle.as_deref().unwrap_or(&resolved.did);

    if !args.yes {
        let kr_record = opake
            .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
            .await?;
        let kr: opake_core::records::Keyring = serde_json::from_value(kr_record.value)?;
        if !kr.members.iter().any(|m| m.did() == resolved.did) {
            anyhow::bail!("{} is not a member of {}", display, args.workspace);
        }

        if kr.members.len() == 1 && kr.members[0].did() == opake.did() {
            anyhow::bail!(
                "you're the only member of {} — delete the workspace instead",
                args.workspace
            );
        }

        eprintln!(
            "removing {} from {} will rotate the workspace key.",
            display, args.workspace
        );
        eprintln!("existing documents stay encrypted under the old key.");
        if !crate::prompt::confirm("continue?")? {
            eprintln!("cancelled");
            return Ok(None);
        }
    }

    let kr_record = opake
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;
    let kr: opake_core::records::Keyring = serde_json::from_value(kr_record.value)?;

    let remaining_dids: Vec<&str> = kr
        .members
        .iter()
        .filter(|m| m.did() != resolved.did)
        .map(|m| m.did())
        .collect();

    let mut remaining_pubkeys = Vec::new();
    for did in &remaining_dids {
        let identity = opake.resolve_identity(did).await?;
        remaining_pubkeys.push(identity.public_key);
    }
    let remaining_keys: Vec<DidMember<'_>> = remaining_dids
        .iter()
        .enumerate()
        .map(|(i, did)| DidMember {
            did,
            public_key: &remaining_pubkeys[i],
        })
        .collect();

    let mut admin = opake.workspace_admin(&workspace);
    let (new_group_key, new_rotation) = admin.remove_member(&resolved.did, &remaining_keys).await?;

    keyring_store::save_group_key(
        &ctx.storage,
        &ctx.did,
        &at_uri.rkey,
        new_rotation,
        &new_group_key,
    )?;

    println!("removed {} from {} (key rotated)", display, args.workspace);
    Ok(None)
}
