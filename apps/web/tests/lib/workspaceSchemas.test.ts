import { describe, expect, it } from "vitest";
import {
  memberAccessLabel,
  missingCurrentWrapLabel,
  toMemberEntry,
} from "@/lib/workspaceSchemas";
import { admitWorkspaceMember, repairWorkspaceMember, type MemberAccessClient } from "@/lib/memberAccessActions";

function memberClient(overrides: Partial<MemberAccessClient> = {}): MemberAccessClient {
  return {
    workspaceMemberApprovalChallenge: async () => new Uint8Array(32),
    workspaceMemberAccessStatus: async () => ({
      did: "did:plc:member",
      hasCurrentWrap: false,
      verification: "unverifiedApprovalRequired",
      canRepair: true,
    }),
    addWorkspaceMember: async () => {},
    repairWorkspaceMemberWrap: async () => {},
    ...overrides,
  };
}

describe("toMemberEntry", () => {
  it("keeps an admitted member visible when their current wrap is absent", () => {
    expect(toMemberEntry({
      did: "did:plc:historical",
      role: "editor",
      unverifiedKeyApproval: new Uint8Array(32),
    })).toEqual({
      did: "did:plc:historical",
      role: "editor",
      hasCurrentWrap: false,
      hasUnverifiedApproval: true,
    });
  });
});

describe("member access presentation", () => {
  it("distinguishes pending approval from a verification refusal", () => {
    expect(memberAccessLabel({
      did: "did:plc:pending",
      hasCurrentWrap: false,
      verification: "unverifiedApprovalRequired",
      canRepair: true,
    })).toContain("approval");
    expect(missingCurrentWrapLabel({
      did: "did:plc:failed",
      hasCurrentWrap: false,
      verification: "verificationError",
      canRepair: true,
    })).toContain("no override");
  });

  it("keeps the historical-only state separate from current access", () => {
    expect(missingCurrentWrapLabel({
      did: "did:plc:historical",
      hasCurrentWrap: false,
      verification: "unverifiedApproved",
      canRepair: true,
    })).toContain("Historical access only");
  });
});

describe("unverified member confirmation", () => {
  it("does not write an admission when the manager cancels", async () => {
    let writes = 0;
    const result = await admitWorkspaceMember(
      memberClient({ addWorkspaceMember: async () => { writes += 1; } }),
      "at://did:plc:manager/at.opake.keyring/workspace",
      "did:plc:member",
      "viewer",
      "member.example",
      () => false,
    );

    expect(result).toBe("cancelled");
    expect(writes).toBe(0);
  });

  it("does not write a repair when the manager cancels", async () => {
    let writes = 0;
    const result = await repairWorkspaceMember(
      memberClient({ repairWorkspaceMemberWrap: async () => { writes += 1; } }),
      "at://did:plc:manager/at.opake.keyring/workspace",
      "did:plc:member",
      () => false,
    );

    expect(result).toBe("cancelled");
    expect(writes).toBe(0);
  });

  it("refuses a verification error without a repair write or override prompt", async () => {
    let writes = 0;
    let prompts = 0;
    const result = await repairWorkspaceMember(
      memberClient({
        workspaceMemberAccessStatus: async () => ({
          did: "did:plc:member",
          hasCurrentWrap: false,
          verification: "verificationError",
          canRepair: true,
        }),
        repairWorkspaceMemberWrap: async () => { writes += 1; },
      }),
      "at://did:plc:manager/at.opake.keyring/workspace",
      "did:plc:member",
      () => { prompts += 1; return true; },
    );

    expect(result).toBe("verificationRefused");
    expect(prompts).toBe(0);
    expect(writes).toBe(0);
  });

  it("repairs an unchanged approved bundle without another prompt", async () => {
    let writes = 0;
    let prompts = 0;
    const result = await repairWorkspaceMember(
      memberClient({
        workspaceMemberAccessStatus: async () => ({
          did: "did:plc:member",
          hasCurrentWrap: false,
          verification: "unverifiedApproved",
          canRepair: true,
        }),
        repairWorkspaceMemberWrap: async () => { writes += 1; },
      }),
      "at://did:plc:manager/at.opake.keyring/workspace",
      "did:plc:member",
      () => { prompts += 1; return false; },
    );

    expect(result).toBe("written");
    expect(prompts).toBe(0);
    expect(writes).toBe(1);
  });
});
