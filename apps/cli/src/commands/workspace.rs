use anyhow::{Context, Result};
#[cfg(test)]
use clap::Parser;
use clap::{Args, Subcommand};
use opake_core::atproto;
use opake_core::client::{ReqwestTransport, Session};
use opake_core::keyrings;
use opake_core::opake::{MemberVerificationStatus, WorkspaceMemberAccessStatus};
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
  opake workspace inspect-member family-photos bob.bsky.social
  opake workspace approve-member family-photos bob.bsky.social --approve-unverified
  opake workspace repair-member family-photos bob.bsky.social
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
    /// Inspect a member's current verification and key access state
    InspectMember(InspectMemberArgs),
    /// Approve an admitted member's exact current unverified key bundle
    ApproveMember(ApproveMemberArgs),
    /// Restore an admitted member's missing current workspace-key wrap
    RepairMember(RepairMemberArgs),
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
    /// Confirm the exact currently resolved unverified encryption bundle for
    /// this admission. Intended for non-interactive use.
    #[arg(long)]
    approve_unverified: bool,
}

#[derive(Args)]
struct InspectMemberArgs {
    /// Workspace name
    workspace: String,
    /// Handle or DID of the admitted member
    member: String,
}

#[derive(Args)]
struct ApproveMemberArgs {
    /// Workspace name
    workspace: String,
    /// Handle or DID of the admitted member
    member: String,
    /// Confirm the exact currently resolved unverified encryption bundle for
    /// this approval. Intended for non-interactive use.
    #[arg(long)]
    approve_unverified: bool,
}

#[derive(Args)]
struct RepairMemberArgs {
    /// Workspace name
    workspace: String,
    /// Handle or DID of the admitted member
    member: String,
    /// Confirm the exact currently resolved unverified encryption bundle for
    /// this repair. Intended for non-interactive use.
    #[arg(long)]
    approve_unverified: bool,
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
            WorkspaceAction::InspectMember(args) => inspect_member(ctx, args).await,
            WorkspaceAction::ApproveMember(args) => approve_member(ctx, args).await,
            WorkspaceAction::RepairMember(args) => repair_member(ctx, args).await,
            WorkspaceAction::Leave(args) => leave(ctx, args).await,
            WorkspaceAction::RemoveMember(args) => remove_member(ctx, args).await,
        }
    }
}

async fn create(ctx: &CommandContext, args: CreateArgs) -> Result<Option<Session>> {
    let mut opake = ctx.opake().await?;
    let created = opake.create_workspace(&args.name, None).await?;

    let at_uri = atproto::parse_at_uri(&created.keyring_uri)?;
    keyring_store::save_group_key(&ctx.storage, &ctx.did, &at_uri.rkey, 0, &created.key)?;

    println!("{} → {}", args.name, created.keyring_uri);
    Ok(None)
}

async fn ls(ctx: &CommandContext, args: LsArgs) -> Result<Option<Session>> {
    let mut opake = ctx.opake().await?;

    // Single source: the indexer sees every keyring the caller is a
    // member of — owned workspaces included, since the owner is always a
    // member of their own. Staleness window exists after `workspace
    // create` until Jetstream delivers the commit to the indexer's firehose consumer.
    let workspaces = opake.discover_member_workspaces().await?;

    if workspaces.is_empty() {
        println!("no workspaces");
        return Ok(None);
    }

    let private_keys = opake.identity().owned_private_keys()?;
    let did = opake.did();
    let bundle = private_keys.bundle();

    for ws in &workspaces {
        let name = keyrings::decrypt_indexer_workspace_name(ws, did, &bundle)
            .unwrap_or_else(|| "<encrypted>".into());
        // The head URI's authority is the DID currently hosting the
        // keyring chain head. For an un-superseded workspace that's the
        // original creator; matches the pre-federation "owner" idea.
        let head_pds_did = atproto::parse_at_uri(&ws.uri)
            .map(|u| u.authority)
            .unwrap_or_default();
        let role_tag = if head_pds_did == did {
            ""
        } else {
            "\t(member)"
        };

        if args.long {
            println!(
                "{}\t{} member(s)\trotation:{}\t{}{}",
                name,
                ws.record.members.len(),
                ws.record.rotation,
                ws.uri,
                role_tag,
            );
        } else {
            println!(
                "{}\t{} member(s){}",
                name,
                ws.record.members.len(),
                role_tag
            );
        }
    }

    println!("\n{} workspace(s)", workspaces.len());
    Ok(None)
}

async fn add_member(ctx: &CommandContext, args: AddMemberArgs) -> Result<Option<Session>> {
    let mut opake = ctx.opake().await?;
    let workspace = opake.resolve_workspace(&args.workspace).await?;
    let current_key = workspace.current_key().context(
        "cannot add a member because this manager does not have the current workspace key; historical reads remain available",
    )?;

    // Pre-resolve for the user-facing display string. Core re-resolves
    // internally (single source of truth for the bundle) — the CLI-side
    // call is just for the success-line handle, not for crypto.
    let transport = ReqwestTransport::new();
    let resolved = resolve::resolve_identity(&transport, &ctx.pds_url, &args.member)
        .await
        .map_err(|error| match error {
            opake_core::error::Error::VerificationFailed(_) => anyhow::anyhow!(
                "{}'s published encryption key failed verification; no override is available.",
                args.member
            ),
            error => error.into(),
        })?;

    let approval = opake
        .workspace_member_approval_challenge(&workspace.id(), &resolved.did)
        .await?;
    let approval = match decide_unverified_approval(
        approval,
        &resolved.did,
        "admission",
        args.approve_unverified,
        crate::prompt::confirm,
    )? {
        ApprovalDecision::Approved(approval) => Some(approval),
        ApprovalDecision::NotRequired => None,
        ApprovalDecision::Cancelled => {
            eprintln!("cancelled");
            return Ok(None);
        }
    };
    opake
        .add_workspace_member(
            &workspace.id(),
            current_key,
            &workspace.historical_keys,
            &resolved.did,
            args.role.clone(),
            approval,
        )
        .await?;

    let display = resolved.handle.as_deref().unwrap_or(&resolved.did);
    println!("added {} to {} ({})", display, args.workspace, args.role);
    Ok(None)
}

async fn inspect_member(ctx: &CommandContext, args: InspectMemberArgs) -> Result<Option<Session>> {
    let mut opake = ctx.opake().await?;
    let workspace = opake.resolve_workspace(&args.workspace).await?;
    let member_did = resolve_member_did(&args.member).await?;
    let status = opake
        .workspace_member_access_status(&workspace.id(), &member_did)
        .await?;
    println!("{}", member_status_line(&status));
    Ok(None)
}

async fn approve_member(ctx: &CommandContext, args: ApproveMemberArgs) -> Result<Option<Session>> {
    let mut opake = ctx.opake().await?;
    let workspace = opake.resolve_workspace(&args.workspace).await?;
    let member_did = resolve_member_did(&args.member).await?;
    let status = opake
        .workspace_member_access_status(&workspace.id(), &member_did)
        .await?;

    let MemberVerificationStatus::UnverifiedApprovalRequired = status.verification else {
        return member_action_refusal(&status, "approve");
    };
    let approval = opake
        .workspace_member_approval_challenge(&workspace.id(), &member_did)
        .await?
        .context("the member no longer has an approvable unverified key bundle; inspect their status again")?;
    let ApprovalDecision::Approved(approval) = decide_unverified_approval(
        Some(approval),
        &member_did,
        "approval",
        args.approve_unverified,
        crate::prompt::confirm,
    )?
    else {
        eprintln!("cancelled");
        return Ok(None);
    };
    opake
        .approve_pending_workspace_member(&workspace.id(), &member_did, approval)
        .await?;
    println!("approved {}'s current unverified key bundle", member_did);
    Ok(None)
}

async fn repair_member(ctx: &CommandContext, args: RepairMemberArgs) -> Result<Option<Session>> {
    let mut opake = ctx.opake().await?;
    let workspace = opake.resolve_workspace(&args.workspace).await?;
    let member_did = resolve_member_did(&args.member).await?;
    let status = opake
        .workspace_member_access_status(&workspace.id(), &member_did)
        .await?;

    if status.has_current_wrap {
        anyhow::bail!("{} already has a current workspace-key wrap", member_did);
    }
    if !status.can_repair {
        anyhow::bail!(
            "cannot repair {} because this manager does not have the current workspace key; the member retains historical access and a manager with the current key can repair it",
            member_did
        );
    }
    let current_key = workspace.current_key().context(
        "cannot repair because the current workspace key is unavailable; historical reads remain available",
    )?;
    let approval = match status.verification {
        MemberVerificationStatus::Verified | MemberVerificationStatus::UnverifiedApproved => None,
        MemberVerificationStatus::UnverifiedApprovalRequired => {
            let approval = opake
                .workspace_member_approval_challenge(&workspace.id(), &member_did)
                .await?
                .context("the member no longer has an approvable unverified key bundle; inspect their status again")?;
            match decide_unverified_approval(
                Some(approval),
                &member_did,
                "repair",
                args.approve_unverified,
                crate::prompt::confirm,
            )? {
                ApprovalDecision::Approved(approval) => Some(approval),
                ApprovalDecision::NotRequired => None,
                ApprovalDecision::Cancelled => {
                    eprintln!("cancelled");
                    return Ok(None);
                }
            }
        }
        MemberVerificationStatus::VerificationError | MemberVerificationStatus::ResolutionError => {
            return member_action_refusal(&status, "repair");
        }
    };
    opake
        .repair_workspace_member_wrap(&workspace.id(), current_key, &member_did, approval)
        .await?;
    println!("repaired {}'s current workspace-key wrap", member_did);
    Ok(None)
}

async fn resolve_member_did(member: &str) -> Result<String> {
    let transport = ReqwestTransport::new();
    let (did, _, _) = resolve::resolve_pds_for_login(&transport, member).await?;
    // The fresh status inspection performs encryption-key verification. This
    // lookup only resolves the stable DID so verification failures remain
    // observable and cannot be mistaken for a different member.
    Ok(did)
}

fn member_status_line(status: &WorkspaceMemberAccessStatus) -> String {
    let verification = match status.verification {
        MemberVerificationStatus::Verified => "verified",
        MemberVerificationStatus::UnverifiedApproved => {
            "unverified current key bundle is already approved"
        }
        MemberVerificationStatus::UnverifiedApprovalRequired => {
            "unverified current key bundle needs explicit approval"
        }
        MemberVerificationStatus::VerificationError => {
            "verification failed; no override is available"
        }
        MemberVerificationStatus::ResolutionError => {
            "current key bundle could not be resolved; no override is available"
        }
    };
    let access = if status.has_current_wrap {
        "current workspace access available"
    } else if status.can_repair {
        "historical access only; this manager can repair the current wrap"
    } else {
        "historical access only; this manager lacks the current workspace key"
    };
    format!("{}: {}; {}", status.did, verification, access)
}

fn member_action_refusal(
    status: &WorkspaceMemberAccessStatus,
    action: &str,
) -> Result<Option<Session>> {
    match status.verification {
        MemberVerificationStatus::Verified => anyhow::bail!(
            "{} is verified and does not need unverified-key approval; use repair if their current wrap is missing",
            status.did
        ),
        MemberVerificationStatus::UnverifiedApproved => anyhow::bail!(
            "{}'s exact current unverified key bundle is already approved; use repair if their current wrap is missing",
            status.did
        ),
        MemberVerificationStatus::UnverifiedApprovalRequired => anyhow::bail!(
            "{} needs explicit approval before it can be repaired",
            status.did
        ),
        MemberVerificationStatus::VerificationError => anyhow::bail!(
            "cannot {} {} because its current key verification failed; no override is available",
            action,
            status.did
        ),
        MemberVerificationStatus::ResolutionError => anyhow::bail!(
            "cannot {} {} because its current key bundle could not be resolved; no override is available",
            action,
            status.did
        ),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ApprovalDecision {
    NotRequired,
    Approved([u8; 32]),
    Cancelled,
}

fn decide_unverified_approval(
    approval: Option<[u8; 32]>,
    did: &str,
    operation: &str,
    approved_by_flag: bool,
    confirm: impl FnOnce(&str) -> Result<bool>,
) -> Result<ApprovalDecision> {
    let Some(approval) = approval else {
        return Ok(ApprovalDecision::NotRequired);
    };
    if approved_by_flag {
        eprintln!(
            "--approve-unverified confirms {}'s exact current unverified encryption bundle for this {} only.",
            did, operation
        );
        return Ok(ApprovalDecision::Approved(approval));
    }
    let confirmation = format!(
        "{} has an unverified encryption key. {} can expose workspace files to someone other than this account. Approve this exact current bundle for this {}?",
        did,
        match operation {
            "admission" => "Admitting them",
            "approval" => "Recording approval",
            _ => "Repairing their workspace access",
        },
        operation,
    );
    if confirm(&confirmation)? {
        Ok(ApprovalDecision::Approved(approval))
    } else {
        Ok(ApprovalDecision::Cancelled)
    }
}

async fn leave(ctx: &CommandContext, args: LeaveArgs) -> Result<Option<Session>> {
    let mut opake = ctx.opake().await?;
    let workspace = opake.resolve_workspace(&args.workspace).await?;

    if !args.yes && !crate::prompt::confirm(&format!("leave workspace {:?}?", args.workspace))? {
        println!("aborted");
        return Ok(None);
    }

    opake.leave_workspace(&workspace.id()).await?;
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

    // The federation `remove_member` path resolves remaining-members'
    // keys + handles the rotation internally; CLI no longer needs the
    // pre-resolution dance the legacy in-place flow required.
    let mut admin = opake.workspace_admin(&workspace);
    let removal = admin.remove_member(&resolved.did).await?;

    keyring_store::save_group_key(
        &ctx.storage,
        &ctx.did,
        &at_uri.rkey,
        removal.rotation,
        &removal.group_key,
    )?;

    println!("removed {} from {} (key rotated)", display, args.workspace);
    if !removal.excluded_members.is_empty() {
        for member in &removal.excluded_members {
            match member.reason {
                opake_core::opake::ExcludedMemberReason::ApprovalRequired => eprintln!(
                    "{} remains admitted but needs a manager to confirm their current unverified keys before repair.",
                    member.did
                ),
                opake_core::opake::ExcludedMemberReason::VerificationFailed => eprintln!(
                    "{} remains admitted but was excluded because verification failed; no override is available.",
                    member.did
                ),
                opake_core::opake::ExcludedMemberReason::ResolutionFailed => eprintln!(
                    "{} remains admitted but could not be resolved for the new key; retry after resolution succeeds.",
                    member.did
                ),
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Parser)]
    struct TestCommand {
        #[command(subcommand)]
        action: WorkspaceAction,
    }

    #[test]
    fn parses_member_commands_and_operation_scoped_consent() {
        let add = TestCommand::try_parse_from([
            "opake",
            "add-member",
            "workspace",
            "did:plc:member",
            "--approve-unverified",
        ])
        .unwrap();
        assert!(matches!(
            add.action,
            WorkspaceAction::AddMember(AddMemberArgs {
                approve_unverified: true,
                ..
            })
        ));

        let approve = TestCommand::try_parse_from([
            "opake",
            "approve-member",
            "workspace",
            "did:plc:member",
            "--approve-unverified",
        ])
        .unwrap();
        assert!(matches!(
            approve.action,
            WorkspaceAction::ApproveMember(ApproveMemberArgs {
                approve_unverified: true,
                ..
            })
        ));

        let repair =
            TestCommand::try_parse_from(["opake", "repair-member", "workspace", "did:plc:member"])
                .unwrap();
        assert!(matches!(
            repair.action,
            WorkspaceAction::RepairMember(RepairMemberArgs {
                approve_unverified: false,
                ..
            })
        ));

        let inspect =
            TestCommand::try_parse_from(["opake", "inspect-member", "workspace", "did:plc:member"])
                .unwrap();
        assert!(matches!(inspect.action, WorkspaceAction::InspectMember(_)));
    }

    #[test]
    fn declined_unverified_consent_returns_cancelled_without_an_approval() {
        let token = [7; 32];
        let decision =
            decide_unverified_approval(Some(token), "did:plc:member", "repair", false, |_| {
                Ok(false)
            })
            .unwrap();
        assert_eq!(decision, ApprovalDecision::Cancelled);
    }

    #[test]
    fn scoped_flag_approves_only_the_current_challenge() {
        let token = [3; 32];
        let decision =
            decide_unverified_approval(Some(token), "did:plc:member", "approval", true, |_| {
                panic!("--approve-unverified must not try to prompt")
            })
            .unwrap();
        assert_eq!(decision, ApprovalDecision::Approved(token));
        assert_eq!(
            decide_unverified_approval(None, "did:plc:member", "repair", true, |_| Ok(true))
                .unwrap(),
            ApprovalDecision::NotRequired
        );
    }

    #[test]
    fn status_words_historical_pending_and_verification_failure() {
        let pending = WorkspaceMemberAccessStatus {
            did: "did:plc:member".into(),
            has_current_wrap: false,
            verification: MemberVerificationStatus::UnverifiedApprovalRequired,
            can_repair: false,
        };
        assert_eq!(
            member_status_line(&pending),
            "did:plc:member: unverified current key bundle needs explicit approval; historical access only; this manager lacks the current workspace key"
        );
        let failed = WorkspaceMemberAccessStatus {
            verification: MemberVerificationStatus::VerificationError,
            ..pending
        };
        assert!(member_status_line(&failed).contains("verification failed; no override"));
    }
}
