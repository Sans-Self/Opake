import type { WorkspaceMemberAccessStatus, WorkspaceRole } from "@opake/sdk";

/** Small injected surface for the member-setting mutations. */
export interface MemberAccessClient {
  workspaceMemberApprovalChallenge(keyringUri: string, memberDid: string): Promise<Uint8Array | null>;
  workspaceMemberAccessStatus(keyringUri: string, memberDid: string): Promise<WorkspaceMemberAccessStatus>;
  addWorkspaceMember(
    keyringUri: string,
    memberDid: string,
    role: WorkspaceRole,
    approval?: Uint8Array,
  ): Promise<unknown>;
  repairWorkspaceMemberWrap(
    keyringUri: string,
    memberDid: string,
    approval?: Uint8Array,
  ): Promise<unknown>;
}

export type MemberWriteResult = "written" | "cancelled" | "verificationRefused" | "currentKeyUnavailable";

/** Prompt before the sole write point for a new unverified admission. */
export async function admitWorkspaceMember(
  client: MemberAccessClient,
  keyringUri: string,
  memberDid: string,
  role: WorkspaceRole,
  displayName: string,
  confirm: (message: string) => boolean,
): Promise<MemberWriteResult> {
  const approval = await client.workspaceMemberApprovalChallenge(keyringUri, memberDid);
  if (approval && !confirm(`${displayName} is unverified. Granting access can expose every workspace file to substituted keys. Continue?`)) {
    return "cancelled";
  }
  await client.addWorkspaceMember(keyringUri, memberDid, role, approval ?? undefined);
  return "written";
}

/**
 * Repair from a fresh inspection. A matching recorded approval reaches the
 * mutation without another prompt; only a current unverified bundle that
 * lacks matching approval asks the manager to decide.
 */
export async function repairWorkspaceMember(
  client: MemberAccessClient,
  keyringUri: string,
  memberDid: string,
  confirm: (message: string) => boolean,
): Promise<MemberWriteResult> {
  const status = await client.workspaceMemberAccessStatus(keyringUri, memberDid);
  if (status.verification === "verificationError" || status.verification === "resolutionError") {
    return "verificationRefused";
  }
  if (!status.canRepair) return "currentKeyUnavailable";

  if (status.verification === "unverifiedApprovalRequired") {
    const approval = await client.workspaceMemberApprovalChallenge(keyringUri, memberDid);
    if (approval && !confirm(`Approve ${memberDid}'s current unverified keys and repair their access?`)) {
      return "cancelled";
    }
    await client.repairWorkspaceMemberWrap(keyringUri, memberDid, approval ?? undefined);
    return "written";
  }

  await client.repairWorkspaceMemberWrap(keyringUri, memberDid);
  return "written";
}
