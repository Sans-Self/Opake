// Web-side view models for workspace member management.
//
// The SDK's `WorkspaceMember` type carries the full wrapped-key structure
// required for crypto operations. UI components only need a flat
// `{ did, role }` view — this module bridges the two and re-exports
// `WorkspaceRole` so components don't reach across the SDK boundary.

import type { WorkspaceMember, WorkspaceMemberAccessStatus, WorkspaceRole } from "@opake/sdk";

export type { WorkspaceRole };

/** Flat member shape for UI rendering — just DID and role, no crypto material. */
export interface KeyringMemberEntry {
  readonly did: string;
  readonly role: WorkspaceRole;
  readonly hasCurrentWrap: boolean;
}

/** Project a raw `WorkspaceMember` onto the UI-facing `KeyringMemberEntry`. */
export function toMemberEntry(member: WorkspaceMember): KeyringMemberEntry {
  return {
    did: member.did,
    role: member.role,
    hasCurrentWrap: member.wrappedKey !== undefined,
  };
}

/** Human-readable, non-colour status for one freshly inspected member. */
export function memberAccessLabel(status: WorkspaceMemberAccessStatus | null): string {
  if (!status) return "Checking current verification";
  switch (status.verification) {
    case "verified":
      return "Verified";
    case "unverifiedApproved":
      return "Unverified — current keys approved";
    case "unverifiedApprovalRequired":
      return "Unverified — approval of current keys required";
    case "verificationError":
      return "Verification error — access changes are refused; no override is available";
    case "resolutionError":
      return "Verification unavailable — access changes are refused until current keys resolve";
    default:
      return "Checking current verification";
  }
}

/** Explain missing-current-wrap state without mistaking it for non-membership. */
export function missingCurrentWrapLabel(status: WorkspaceMemberAccessStatus | null): string {
  if (!status) return "Historical access only — checking current verification before repair.";
  switch (status.verification) {
    case "verified":
      return "Historical access only — this member cannot read new files until a manager repairs access.";
    case "unverifiedApproved":
      return "Historical access only — their current unverified keys are approved, but a manager must repair access.";
    case "unverifiedApprovalRequired":
      return "Historical access only — a manager must approve their current unverified keys before repair.";
    case "verificationError":
      return "Historical access only — verification failed. Access changes are refused; no override is available.";
    case "resolutionError":
      return "Historical access only — current keys could not be resolved. Access changes are refused until they resolve.";
    default:
      return "Historical access only — checking current verification before repair.";
  }
}
