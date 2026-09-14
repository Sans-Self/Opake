// Member exclusion and repair through production CLI/PDS/indexer paths. This
// deliberately changes an ordinary unverified actor's published bundle: the
// PDS, resolver, indexer, and keyring writer are all the real dev-env ones.
// The test restores the actor record in a finally block, so namespaces remain
// disposable and default fixtures retain their unverified baseline.

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { actorNamespace, actorsFor, type Actor } from "../../e2e/namespace.js";
import {
  deleteRepositoryRecord,
  publishedPublicKey,
  putRepositoryRecord,
  repositoryRecord,
} from "../../e2e/pds-admin.js";
import { testEnv } from "../../helpers/pds.js";
import {
  actorByName,
  actorOnPds,
  cli,
  cliApprovingUnverified,
  headUri,
  login,
  memberCount,
  pollUntil,
  rotationCount,
  rkeyOf,
  startCli,
  stackIsUp,
  stopCli,
  uploadTextToWorkspace,
  workspaceListed,
} from "../../helpers/devenv.js";

const OWNER = actorOnPds("pds-a").name; // alice
const MEMBER = actorOnPds("pds-b").name; // carol
const REMOVED = actorByName("eve").name; // pds-c

// eslint-disable-next-line functional/no-let
let seq = 0;
const uniqueName = (tag: string): string => `member-repair-${tag}-${Date.now()}-${seq++}`;

// eslint-disable-next-line functional/no-let
let memberDid = "";
// eslint-disable-next-line functional/no-let
let removedDid = "";

function actor(name: string): Actor {
  const found = actorsFor(actorNamespace()).find((candidate) => candidate.name === name);
  if (!found) throw new Error(`missing fixture actor ${name}`);
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

function memberInHead(value: unknown, did: string): Record<string, unknown> {
  const members = recordObject(value).members;
  if (!Array.isArray(members)) throw new Error("keyring head has no member list");
  const member = members.find((candidate) => recordObject(candidate).did === did);
  if (!member) throw new Error(`head does not retain ${did}`);
  return recordObject(member);
}

function historicalMemberInHead(value: unknown, did: string): Record<string, unknown> {
  const history = recordObject(value).keyHistory;
  if (!Array.isArray(history)) throw new Error("keyring head has no key history");
  const members = history.flatMap((entry) => {
    const value = recordObject(entry).members;
    return Array.isArray(value) ? value : [];
  });
  const member = members.find((candidate) => recordObject(candidate).did === did);
  if (!member) throw new Error(`key history does not retain ${did}`);
  return recordObject(member);
}

async function currentHeadRecord(
  workspace: string,
): Promise<{ readonly uri: string; readonly value: unknown }> {
  const uri = await headUri(OWNER, workspace);
  if (!uri) throw new Error(`no indexed keyring head for ${workspace}`);
  const record = await repositoryRecord(actor(OWNER), "at.opake.keyring", rkeyOf(uri));
  if (!record) throw new Error(`PDS has no keyring head ${uri}`);
  return { uri, value: record.value };
}

// This scenario intentionally rewrites an actor's public-key record and
// creates a keyring chain. It only runs in a disposable namespace, never
// against the checked-in default population.
const skipMutationSuite = testEnv() !== "devenv" || actorNamespace() === "";

describe.skipIf(skipMutationSuite)("member exclusion and repair", () => {
  beforeAll(async () => {
    if (!(await stackIsUp())) {
      throw new Error(
        "dev-env stack is not up — run `just dev-env-up` before `just e2e-federation`",
      );
    }
    await startCli();
    await login(OWNER);
    memberDid = await login(MEMBER);
    removedDid = await login(REMOVED);
  }, 180_000);

  afterAll(async () => {
    await stopCli();
  });

  it("retains changed-key membership/history, requires scoped approval, repairs, and rolls back safely", async () => {
    const workspace = uniqueName("changed-key");
    expect((await cli(OWNER, ["workspace", "create", workspace])).code).toBe(0);
    expect(await pollUntil(() => workspaceListed(OWNER, workspace))).toBe(true);
    const genesisUri = await headUri(OWNER, workspace);
    if (!genesisUri) throw new Error(`no indexed genesis keyring for ${workspace}`);

    // Both counterparty admissions are explicit per-operation acknowledgements;
    // ordinary fixtures remain unverified by design.
    const addedMember = await cliApprovingUnverified(OWNER, [
      "workspace",
      "add-member",
      workspace,
      memberDid,
    ]);
    expect(addedMember.code, addedMember.stderr).toBe(0);
    // The next admission resolves this new indexed head. Otherwise two
    // supersedes can fork from the same predecessor and lose a member.
    expect(await pollUntil(async () => (await memberCount(OWNER, workspace)) === 2)).toBe(true);
    const addedRemoved = await cliApprovingUnverified(OWNER, [
      "workspace",
      "add-member",
      workspace,
      removedDid,
    ]);
    expect(addedRemoved.code, addedRemoved.stderr).toBe(0);
    expect(await pollUntil(async () => (await memberCount(OWNER, workspace)) === 3)).toBe(true);

    const historicalMarker = `historical-${Date.now()}-${seq++}`;
    const historicalDocument = await uploadTextToWorkspace(
      OWNER,
      workspace,
      "historical.txt",
      historicalMarker,
    );

    const originalPublicKey = await publishedPublicKey(actor(MEMBER));
    if (!originalPublicKey) throw new Error("member fixture has no public-key record");
    await putRepositoryRecord(
      actor(MEMBER),
      "at.opake.publicKey",
      "self",
      changedUnverifiedBundle(originalPublicKey),
    );

    try {
      // spec:workspace-membership § Removal rotates the group key; leave does not
      const removal = await cli(OWNER, ["workspace", "remove-member", workspace, removedDid, "-y"]);
      expect(removal.code, removal.stderr).toBe(0);
      expect(removal.stderr).toContain(memberDid);
      expect(removal.stderr).toContain(
        "needs a manager to confirm their current unverified keys before repair",
      );
      expect(await pollUntil(async () => (await rotationCount(OWNER, workspace)) === 1)).toBe(true);
      expect(await pollUntil(async () => (await memberCount(OWNER, workspace)) === 2)).toBe(true);

      const excluded = await currentHeadRecord(workspace);
      // Every current supersede must carry the immutable genesis identity.
      // This catches a PDS/indexer fixture that silently drops the field before
      // a historical-only client can be expected to unwrap history safely.
      expect(recordObject(excluded.value).lineage).toBe(genesisUri);
      const currentMember = memberInHead(excluded.value, memberDid);
      expect(currentMember.wrappedKey).toBeUndefined();
      expect(currentMember.unverifiedKeyApproval).toBeDefined();
      expect(historicalMemberInHead(excluded.value, memberDid).wrappedKey).toBeDefined();
      const excludedApproval = JSON.stringify(currentMember.unverifiedKeyApproval);

      // Missing current access does not erase the prior rotation: Carol's
      // original private key still opens content from before the removal.
      const historicalRead = await cli(MEMBER, [
        "download",
        "--workspace-member",
        historicalDocument,
        "--stdout",
      ]);
      expect(historicalRead.code, historicalRead.stderr).toBe(0);
      expect(historicalRead.stdout).toBe(historicalMarker);

      // New content is under rotation one, whose wrap was deliberately
      // withheld. A current-key read must fail until a manager repairs it.
      const currentDocument = await uploadTextToWorkspace(
        OWNER,
        workspace,
        "current.txt",
        `current-${Date.now()}-${seq++}`,
      );
      const currentRead = await cli(MEMBER, [
        "download",
        "--workspace-member",
        currentDocument,
        "--stdout",
      ]);
      expect(currentRead.code).not.toBe(0);

      const pending = await cli(OWNER, ["workspace", "inspect-member", workspace, memberDid]);
      expect(pending.code, pending.stderr).toBe(0);
      expect(pending.stdout).toContain("needs explicit approval");
      expect(pending.stdout).toContain("historical access only");

      const approved = await cliApprovingUnverified(OWNER, [
        "workspace",
        "approve-member",
        workspace,
        memberDid,
      ]);
      expect(approved.code, approved.stderr).toBe(0);
      expect(
        await pollUntil(async () => {
          const head = await currentHeadRecord(workspace);
          return (
            head.uri !== excluded.uri &&
            JSON.stringify(memberInHead(head.value, memberDid).unverifiedKeyApproval) !== excludedApproval
          );
        }),
      ).toBe(true);
      const approvedHead = await currentHeadRecord(workspace);
      const approval = memberInHead(approvedHead.value, memberDid).unverifiedKeyApproval;
      expect(approval).toBeDefined();

      // The just-recorded commitment exactly matches the changed bundle, so
      // repair needs no second prompt and keeps the rotation at one.
      const repaired = await cli(OWNER, ["workspace", "repair-member", workspace, memberDid]);
      expect(repaired.code, repaired.stderr).toBe(0);
      expect(await pollUntil(async () => (await rotationCount(OWNER, workspace)) === 1)).toBe(true);
      expect(
        await pollUntil(async () => {
          const head = await currentHeadRecord(workspace);
          return head.uri !== approvedHead.uri && memberInHead(head.value, memberDid).wrappedKey !== undefined;
        }),
      ).toBe(true);
      const repairedHead = await currentHeadRecord(workspace);
      expect(memberInHead(repairedHead.value, memberDid).wrappedKey).toBeDefined();

      // Deleting the repair head restores the approval-only predecessor. The
      // restored head supplies its own approval and missing-wrap state.
      await deleteRepositoryRecord(actor(OWNER), "at.opake.keyring", rkeyOf(repairedHead.uri));
      expect(
        await pollUntil(async () => (await headUri(OWNER, workspace)) === approvedHead.uri),
      ).toBe(true);
      const restored = await currentHeadRecord(workspace);
      const restoredMember = memberInHead(restored.value, memberDid);
      expect(restoredMember.wrappedKey).toBeUndefined();
      expect(restoredMember.unverifiedKeyApproval).toEqual(approval);

      // Restore Carol's real private-key bundle, approve it as the newly
      // current unverified bundle, then repair and prove native cross-PDS
      // access uses the repaired current wrap.
      await putRepositoryRecord(actor(MEMBER), "at.opake.publicKey", "self", originalPublicKey);
      const reapproved = await cliApprovingUnverified(OWNER, [
        "workspace",
        "approve-member",
        workspace,
        memberDid,
      ]);
      expect(reapproved.code, reapproved.stderr).toBe(0);
      expect(
        await pollUntil(async () => {
          const head = await currentHeadRecord(workspace);
          return (
            head.uri !== restored.uri &&
            JSON.stringify(memberInHead(head.value, memberDid).unverifiedKeyApproval) !==
              JSON.stringify(restoredMember.unverifiedKeyApproval)
          );
        }),
      ).toBe(true);
      const reapprovedHead = await currentHeadRecord(workspace);
      const repairedRealKey = await cli(OWNER, [
        "workspace",
        "repair-member",
        workspace,
        memberDid,
      ]);
      expect(repairedRealKey.code, repairedRealKey.stderr).toBe(0);
      expect(
        await pollUntil(async () => {
          const head = await currentHeadRecord(workspace);
          return (
            head.uri !== reapprovedHead.uri &&
            memberInHead(head.value, memberDid).wrappedKey !== undefined
          );
        }),
      ).toBe(true);

      const marker = `repaired-${Date.now()}-${seq++}`;
      const document = await uploadTextToWorkspace(OWNER, workspace, "repaired.txt", marker);
      const readable = await cli(MEMBER, ["download", "--workspace-member", document, "--stdout"]);
      expect(readable.code, readable.stderr).toBe(0);
      expect(readable.stdout).toBe(marker);
    } finally {
      await putRepositoryRecord(actor(MEMBER), "at.opake.publicKey", "self", originalPublicKey);
    }
  }, 360_000);
});
