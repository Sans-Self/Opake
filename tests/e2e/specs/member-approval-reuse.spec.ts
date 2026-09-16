// A CLI-recorded approval is relationship state, not a UI-local consent bit:
// a newly opened web manager must reuse it for repair without prompting again.

import type { Browser, Dialog, Page } from "@playwright/test";
import {
  blockadeTest as test,
  expect,
  ACTORS,
  authFile,
  installBlockade,
  type Actor,
} from "../fixtures";
import { actorNamespace } from "../namespace";
import { publishedPublicKey, putRepositoryRecord, repositoryRecord } from "../pds-admin";
import {
  cli,
  cliApprovingUnverified,
  headUri,
  login,
  memberCount,
  pollUntil,
  startCli,
  stopCli,
  uploadTextToWorkspace,
  workspaceListed,
} from "../../helpers/devenv";

const OWNER = "alice";
const MEMBER = "carol";
const REMOVED = "eve";

const uniqueName = (tag: string): string => `approval-reuse-${tag}-${Date.now().toString(36)}`;

function actor(name: string): Actor {
  const found = ACTORS.find((candidate) => candidate.name === name);
  if (!found) throw new Error(`fixture actor ${name} not found`);
  return found;
}

function recordObject(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("expected a record object");
  }
  return value as Record<string, unknown>;
}

function changedUnverifiedBundle(record: unknown): Record<string, unknown> {
  const changed = structuredClone(recordObject(record));
  const x25519 = recordObject(changed.x25519PublicKey);
  const encoded = x25519.$bytes;
  if (typeof encoded !== "string") throw new Error("public-key record has no X25519 bytes");
  const bytes = Buffer.from(encoded, "base64");
  bytes[0] = (bytes[0] ?? 0) ^ 1;
  x25519.$bytes = bytes.toString("base64");
  return changed;
}

const rkeyOf = (uri: string): string => uri.slice(uri.lastIndexOf("/") + 1);

function memberInHead(value: unknown, did: string): Record<string, unknown> {
  const members = recordObject(value).members;
  if (!Array.isArray(members)) throw new Error("keyring head has no member list");
  const member = members.find((candidate) => recordObject(candidate).did === did);
  if (!member) throw new Error(`keyring head has no member ${did}`);
  return recordObject(member);
}

async function freshOwnerPage(
  browser: Browser,
): Promise<{ page: Page; close: () => Promise<void> }> {
  const context = await browser.newContext({
    storageState: authFile(OWNER),
    ignoreHTTPSErrors: true,
  });
  const page = await context.newPage();
  const assertNoEscape = await installBlockade(page);
  return {
    page,
    close: async () => {
      assertNoEscape();
      await context.close();
    },
  };
}

test.skip(actorNamespace() === "", "approval-reuse mutates only a disposable namespace");

test("a fresh web manager repairs a CLI-approved unchanged bundle without another confirmation", async ({
  browser,
}) => {
  test.setTimeout(300_000);

  const workspace = uniqueName("workspace");
  const member = actor(MEMBER);
  let originalPublicKey: unknown | null = null;
  let web: { page: Page; close: () => Promise<void> } | null = null;

  await startCli();
  try {
    const memberDid = await login(MEMBER);
    const removedDid = await login(REMOVED);
    await login(OWNER);

    expect((await cli(OWNER, ["workspace", "create", workspace])).code).toBe(0);
    expect(await pollUntil(() => workspaceListed(OWNER, workspace))).toBe(true);

    const admittedMember = await cliApprovingUnverified(OWNER, [
      "workspace",
      "add-member",
      workspace,
      memberDid,
    ]);
    expect(admittedMember.code, admittedMember.stderr).toBe(0);
    expect(await pollUntil(async () => (await memberCount(OWNER, workspace)) === 2)).toBe(true);
    const admissionHeadUri = await headUri(OWNER, workspace);
    if (!admissionHeadUri) throw new Error("no indexed admission head");
    const admissionHead = await repositoryRecord(
      actor(OWNER),
      "at.opake.keyring",
      rkeyOf(admissionHeadUri),
    );
    if (!admissionHead) throw new Error("PDS omitted indexed admission head");
    const admissionApproval = memberInHead(admissionHead.value, memberDid).unverifiedKeyApproval;
    expect(admissionApproval).toBeDefined();
    const admittedRemoved = await cliApprovingUnverified(OWNER, [
      "workspace",
      "add-member",
      workspace,
      removedDid,
    ]);
    expect(admittedRemoved.code, admittedRemoved.stderr).toBe(0);
    expect(await pollUntil(async () => (await memberCount(OWNER, workspace)) === 3)).toBe(true);

    const historicalMarker = uniqueName("historical");
    const historicalDocument = await uploadTextToWorkspace(
      OWNER,
      workspace,
      "historical.txt",
      historicalMarker,
    );

    originalPublicKey = await publishedPublicKey(member);
    if (!originalPublicKey) throw new Error("member fixture has no public-key record");
    await putRepositoryRecord(
      member,
      "at.opake.publicKey",
      "self",
      changedUnverifiedBundle(originalPublicKey),
    );

    const removal = await cli(OWNER, ["workspace", "remove-member", workspace, removedDid, "-y"]);
    expect(removal.code, removal.stderr).toBe(0);
    expect(await pollUntil(async () => (await memberCount(OWNER, workspace)) === 2)).toBe(true);
    const exclusionHeadUri = await headUri(OWNER, workspace);
    if (!exclusionHeadUri || exclusionHeadUri === admissionHeadUri) {
      throw new Error("changed-key exclusion did not reach a new indexed head");
    }
    const exclusionHead = await repositoryRecord(
      actor(OWNER),
      "at.opake.keyring",
      rkeyOf(exclusionHeadUri),
    );
    if (!exclusionHead) throw new Error("PDS omitted indexed exclusion head");
    const excludedMember = memberInHead(exclusionHead.value, memberDid);
    const exclusionRotation = recordObject(exclusionHead.value).rotation;
    expect(excludedMember.wrappedKey).toBeUndefined();
    expect(excludedMember.unverifiedKeyApproval).toEqual(admissionApproval);

    const historicalRead = await cli(MEMBER, [
      "download",
      "--workspace-member",
      historicalDocument,
      "--stdout",
    ]);
    expect(historicalRead.code, historicalRead.stderr).toBe(0);
    expect(historicalRead.stdout).toBe(historicalMarker);

    // Restore the actual member bundle before repair. The approval written by
    // the native CLI admission is still on the relationship and now matches
    // again, so the fresh web manager must not ask for a second confirmation.
    await putRepositoryRecord(member, "at.opake.publicKey", "self", originalPublicKey);
    expect(
      await pollUntil(async () => {
        const status = await cli(OWNER, ["workspace", "inspect-member", workspace, memberDid]);
        return status.code === 0 && status.stdout.includes("already approved");
      }),
    ).toBe(true);

    web = await freshOwnerPage(browser);
    await web.page.goto("/cabinet/files");
    const workspaceLink = web.page.getByRole("link", { name: workspace });
    await expect(workspaceLink).toBeVisible({ timeout: 60_000 });
    await workspaceLink.click();
    await web.page.getByRole("link", { name: "Workspace settings" }).click();

    // The fresh manager's own opportunistic daemon may repair the wrap from
    // the recorded approval before a person reaches the button; both routes
    // consume the same evidence and neither may prompt. Accept whichever
    // lands first, but if the button is offered, drive it.
    const confirmations: Dialog[] = [];
    const recordUnexpectedConfirmation = (dialog: Dialog) => {
      confirmations.push(dialog);
      void dialog.dismiss();
    };
    web.page.on("dialog", recordUnexpectedConfirmation);
    const repair = web.page.getByRole("button", { name: "Repair access", exact: true });
    const memberRow = web.page.getByRole("button", { name: `Remove ${memberDid}` });
    await expect(repair.or(memberRow)).toBeVisible({ timeout: 60_000 });
    if (await repair.isVisible().catch(() => false)) {
      await repair.click();
      await expect(web.page.getByText("Member access repaired").first()).toBeVisible({
        timeout: 60_000,
      });
    }
    web.page.off("dialog", recordUnexpectedConfirmation);
    expect(confirmations).toHaveLength(0);
    expect(
      await pollUntil(async () => {
        const currentHeadUri = await headUri(OWNER, workspace);
        if (!currentHeadUri || currentHeadUri === exclusionHeadUri) return false;
        const head = await repositoryRecord(
          actor(OWNER),
          "at.opake.keyring",
          rkeyOf(currentHeadUri),
        );
        if (!head) return false;
        const repairedMember = memberInHead(head.value, memberDid);
        return (
          repairedMember.wrappedKey !== undefined &&
          recordObject(head.value).rotation === exclusionRotation &&
          JSON.stringify(repairedMember.unverifiedKeyApproval) === JSON.stringify(admissionApproval)
        );
      }),
    ).toBe(true);
    const repairedHeadUri = await headUri(OWNER, workspace);
    if (!repairedHeadUri) throw new Error("no indexed repaired head");
    const repairedHead = await repositoryRecord(
      actor(OWNER),
      "at.opake.keyring",
      rkeyOf(repairedHeadUri),
    );
    if (!repairedHead) throw new Error("PDS omitted indexed repaired head");
    const repairedMember = memberInHead(repairedHead.value, memberDid);
    expect(repairedMember.wrappedKey).toBeDefined();
    expect(recordObject(repairedHead.value).rotation).toBe(exclusionRotation);
    expect(repairedMember.unverifiedKeyApproval).toEqual(admissionApproval);

    const repairedMarker = uniqueName("repaired");
    const repairedDocument = await uploadTextToWorkspace(
      OWNER,
      workspace,
      "repaired.txt",
      repairedMarker,
    );
    const repairedRead = await cli(MEMBER, [
      "download",
      "--workspace-member",
      repairedDocument,
      "--stdout",
    ]);
    expect(repairedRead.code, repairedRead.stderr).toBe(0);
    expect(repairedRead.stdout).toBe(repairedMarker);
  } finally {
    if (originalPublicKey) {
      await putRepositoryRecord(member, "at.opake.publicKey", "self", originalPublicKey);
    }
    if (web) await web.close();
    await stopCli();
  }
});
