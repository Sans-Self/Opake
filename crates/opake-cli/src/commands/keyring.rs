use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Args, Subcommand};
use opake_core::atproto;
use opake_core::client::Session;
use opake_core::crypto::OsRng;
use opake_core::keyrings::{self, CreateKeyringParams, MemberKey};
use opake_core::resolve;

use crate::commands::Execute;
use crate::identity;
use crate::keyring_store;
use crate::session::{self, CommandContext};
use opake_core::client::ReqwestTransport;

#[derive(Args)]
/// Manage keyrings for group-based access control
pub struct KeyringCommand {
    #[command(subcommand)]
    action: KeyringAction,
}

#[derive(Subcommand)]
enum KeyringAction {
    /// Create a new keyring
    Create(CreateArgs),
    /// List keyrings
    Ls(LsArgs),
    /// Add a member to a keyring
    AddMember(AddMemberArgs),
    /// Remove a member from a keyring (rotates group key)
    RemoveMember(RemoveMemberArgs),
}

#[derive(Args)]
struct CreateArgs {
    /// Name for the keyring (e.g. "family-photos")
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
    /// Keyring name
    keyring: String,
    /// Handle or DID of the new member
    member: String,
}

#[derive(Args)]
struct RemoveMemberArgs {
    /// Keyring name
    keyring: String,
    /// Handle or DID of the member to remove
    member: String,
    /// Skip confirmation prompt
    #[arg(short, long)]
    yes: bool,
}

impl Execute for KeyringCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        match self.action {
            KeyringAction::Create(args) => create(ctx, args).await,
            KeyringAction::Ls(args) => ls(ctx, args).await,
            KeyringAction::AddMember(args) => add_member(ctx, args).await,
            KeyringAction::RemoveMember(args) => remove_member(ctx, args).await,
        }
    }
}

async fn create(ctx: &CommandContext, args: CreateArgs) -> Result<Option<Session>> {
    let mut client = session::load_client(&ctx.storage, &ctx.did)?;
    let id = identity::load_identity(&ctx.storage, &ctx.did)?;
    let owner_pubkey = id.public_key_bytes()?;

    let params = CreateKeyringParams {
        name: &args.name,
        owner_did: &id.did,
        owner_public_key: &owner_pubkey,
        created_at: &Utc::now().to_rfc3339(),
    };

    let (uri, group_key) = keyrings::create_keyring(&mut client, &params, &mut OsRng).await?;

    let at_uri = atproto::parse_at_uri(&uri)?;
    keyring_store::save_group_key(&ctx.storage, &ctx.did, &at_uri.rkey, 0, &group_key)?;

    println!("{} → {}", args.name, uri);
    Ok(session::refreshed_session(&client))
}

async fn ls(ctx: &CommandContext, args: LsArgs) -> Result<Option<Session>> {
    let mut client = session::load_client(&ctx.storage, &ctx.did)?;
    let entries = keyrings::list_keyrings(&mut client).await?;

    if entries.is_empty() {
        println!("no keyrings");
        return Ok(session::refreshed_session(&client));
    }

    for entry in &entries {
        if args.long {
            println!(
                "{}\t{} member(s)\trotation:{}\t{}",
                entry.name, entry.member_count, entry.rotation, entry.uri,
            );
        } else {
            println!("{}\t{} member(s)", entry.name, entry.member_count);
        }
    }

    println!("\n{} keyring(s)", entries.len());
    Ok(session::refreshed_session(&client))
}

async fn add_member(ctx: &CommandContext, args: AddMemberArgs) -> Result<Option<Session>> {
    let mut client = session::load_client(&ctx.storage, &ctx.did)?;

    let entry = keyrings::resolve_keyring_uri(&mut client, &args.keyring).await?;
    let at_uri = atproto::parse_at_uri(&entry.uri)?;

    let group_key =
        keyring_store::load_group_key(&ctx.storage, &ctx.did, &at_uri.rkey, entry.rotation)?;

    let transport = ReqwestTransport::new();
    let resolved = resolve::resolve_identity(&transport, &ctx.pds_url, &args.member).await?;

    keyrings::add_member(
        &mut client,
        &entry.uri,
        &group_key,
        &resolved.did,
        &resolved.public_key,
        &Utc::now().to_rfc3339(),
        &mut OsRng,
    )
    .await?;

    let display = resolved.handle.as_deref().unwrap_or(&resolved.did);
    println!("added {} to {}", display, args.keyring);
    Ok(session::refreshed_session(&client))
}

async fn remove_member(ctx: &CommandContext, args: RemoveMemberArgs) -> Result<Option<Session>> {
    let mut client = session::load_client(&ctx.storage, &ctx.did)?;

    let entry = keyrings::resolve_keyring_uri(&mut client, &args.keyring).await?;
    let at_uri = atproto::parse_at_uri(&entry.uri)?;

    // Resolve the member to remove — we need their DID
    let transport = ReqwestTransport::new();
    let resolved = resolve::resolve_identity(&transport, &ctx.pds_url, &args.member).await?;
    let display = resolved.handle.as_deref().unwrap_or(&resolved.did);

    if !args.yes {
        let id =
            identity::load_identity(&ctx.storage, &ctx.did).context("run `opake login` first")?;

        // Check they're actually a member before prompting
        let kr_record = client
            .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
            .await?;
        let kr: opake_core::records::Keyring = serde_json::from_value(kr_record.value)?;
        if !kr.members.iter().any(|m| m.did == resolved.did) {
            anyhow::bail!("{} is not a member of {}", display, args.keyring);
        }

        // Don't allow removing yourself if you're the only member
        if kr.members.len() == 1 && kr.members[0].did == id.did {
            anyhow::bail!(
                "you're the only member of {} — delete the keyring instead",
                args.keyring
            );
        }

        eprintln!(
            "removing {} from {} will rotate the group key.",
            display, args.keyring
        );
        eprintln!("existing documents stay encrypted under the old key.");
        eprint!("continue? [y/N] ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            eprintln!("cancelled");
            return Ok(session::refreshed_session(&client));
        }
    }

    // Collect remaining members' public keys by resolving each one
    // (we need fresh pubkeys to wrap the new group key)
    let kr_record = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;
    let kr: opake_core::records::Keyring = serde_json::from_value(kr_record.value)?;

    let remaining_dids: Vec<String> = kr
        .members
        .iter()
        .filter(|m| m.did != resolved.did)
        .map(|m| m.did.clone())
        .collect();

    let mut remaining_keys = Vec::new();
    let mut remaining_pubkeys = Vec::new();
    for did in &remaining_dids {
        let identity = resolve::resolve_identity(&transport, &ctx.pds_url, did).await?;
        remaining_pubkeys.push(identity.public_key);
    }
    for (i, did) in remaining_dids.iter().enumerate() {
        remaining_keys.push(MemberKey {
            did,
            public_key: &remaining_pubkeys[i],
        });
    }

    let (new_group_key, new_rotation) = keyrings::remove_member(
        &mut client,
        &entry.uri,
        &resolved.did,
        &remaining_keys,
        &Utc::now().to_rfc3339(),
        &mut OsRng,
    )
    .await?;

    keyring_store::save_group_key(
        &ctx.storage,
        &ctx.did,
        &at_uri.rkey,
        new_rotation,
        &new_group_key,
    )?;

    println!("removed {} from {} (key rotated)", display, args.keyring);
    Ok(session::refreshed_session(&client))
}
