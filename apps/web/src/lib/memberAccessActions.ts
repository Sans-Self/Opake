import type { WorkspaceMemberAccessStatus, WorkspaceMemberWriteResult, WorkspaceRole } from "@opake/sdk";

/** Small injected surface for the member-setting mutations. */
export interface MemberAccessClient {
  workspaceMemberApprovalChallenge(keyringUri: string, memberDid: string): Promise<Uint8Array | null>;
  workspaceMemberAccessStatus(keyringUri: string, memberDid: string): Promise<WorkspaceMemberAccessStatus>;
  addWorkspaceMember(
    keyringUri: string,
    memberDid: string,
    role: WorkspaceRole,
    approval?: Uint8Array,
  ): Promise<WorkspaceMemberWriteResult | undefined>;
  repairWorkspaceMemberWrap(
    keyringUri: string,
    memberDid: string,
    approval?: Uint8Array,
  ): Promise<WorkspaceMemberWriteResult | undefined>;
}

export type MemberWriteResult =
  | { readonly status: "written"; readonly verification?: WorkspaceMemberWriteResult["verificationNotice"] }
  | { readonly status: "cancelled" | "verificationRefused" | "currentKeyUnavailable" };

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
    return { status: "cancelled" };
  }
  const write = await client.addWorkspaceMember(keyringUri, memberDid, role, approval ?? undefined);
  return { status: "written", verification: write?.verificationNotice };
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
    return { status: "verificationRefused" };
  }
  if (!status.canRepair) return { status: "currentKeyUnavailable" };

  if (status.verification === "unverifiedApprovalRequired") {
    const approval = await client.workspaceMemberApprovalChallenge(keyringUri, memberDid);
    if (approval && !confirm(`Approve ${memberDid}'s current unverified keys and repair their access?`)) {
      return { status: "cancelled" };
    }
    const write = await client.repairWorkspaceMemberWrap(keyringUri, memberDid, approval ?? undefined);
    return { status: "written", verification: write?.verificationNotice };
  }

  const write = await client.repairWorkspaceMemberWrap(keyringUri, memberDid);
  return { status: "written", verification: write?.verificationNotice };
}
