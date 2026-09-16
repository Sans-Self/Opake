// Pending share queue: queue, list, retry, cancel, expiry.

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { startPds, stopPds, getPds } from "../../helpers/pds.js";
import { setupAccount } from "../../helpers/account.js";
import { opake } from "../../helpers/cli.js";

// eslint-disable-next-line functional/no-let
let alice: Awaited<ReturnType<typeof setupAccount>>;

const tempDirs: string[] = [];

function freshDir(): string {
  const dir = mkdtempSync(join(tmpdir(), "opake-e2e-pending-"));
  tempDirs.push(dir);
  return dir;
}

function aliceOpake(args: readonly string[]) {
  return opake(args, {
    configDir: alice.configDir,
    env: { OPAKE_PLC_DIRECTORY: getPds().url },
  });
}

beforeAll(async () => {
  await startPds();
  alice = await setupAccount("did:plc:alice", "alice.test");
});

afterAll(async () => {
  await stopPds();
  for (const dir of tempDirs) {
    rmSync(dir, { recursive: true, force: true });
  }
  rmSync(alice.configDir, { recursive: true, force: true });
});

describe("pending shares", () => {
  it("queues share when recipient has no public key", async () => {
    const workDir = freshDir();
    const testFile = join(workDir, "pending-test.txt");
    writeFileSync(testFile, "pending content");
    await aliceOpake(["upload", testFile]);

    // Create a bare account (DID resolves, no publicKey/self)
    const pds = getPds();
    pds.createAccount("nopake.test");

    const share = await aliceOpake([
      "share",
      "new",
      "pending-test.txt",
      "nopake.test",
    ]);
    expect(share.code).toBe(0);
    expect(share.stdout).toContain("hasn't set up Opake yet");
    expect(share.stdout).toContain("queued");
  });

  it("lists pending shares", async () => {
    const pending = await aliceOpake(["share", "pending"]);
    expect(pending.code).toBe(0);
    expect(pending.stdout).toContain("nopake.test");
    expect(pending.stdout).toContain("at.opake.document");
  });

  it("retries pending shares (still pending when recipient not ready)", async () => {
    const retry = await aliceOpake(["share", "retry"]);
    expect(retry.code).toBe(0);
    expect(retry.stdout).toContain("still pending");
  });

  it("completes pending share when recipient sets up", async () => {
    const pds = getPds();

    // Set up charlie with a public key
    // The DID must match what createAccount generated: did:plc:{handle with dots replaced by dashes}
    const nopakeCtx = await setupAccount("did:plc:nopake-test", "nopake.test");
    tempDirs.push(nopakeCtx.configDir);

    // Retry — should complete now
    const retry = await aliceOpake(["share", "retry"]);
    expect(retry.code).toBe(0);
    expect(retry.stdout).toContain("completed");

    // Pending list should be empty
    const pending = await aliceOpake(["share", "pending"]);
    expect(pending.code).toBe(0);
    expect(pending.stdout).toContain("no pending shares");

    // Grant should exist
    const grants = pds.listRecords("did:plc:alice", "at.opake.grant");
    const charlieGrants = grants.filter(
      (r) => (r.value as { recipient?: string }).recipient === "did:plc:nopake-test",
    );
    expect(charlieGrants.length).toBeGreaterThanOrEqual(1);
  });

  it("cancels a pending share", async () => {
    const workDir = freshDir();
    const testFile = join(workDir, "cancel-test.txt");
    writeFileSync(testFile, "will be cancelled");
    await aliceOpake(["upload", testFile]);

    // Queue a share with dave (no public key)
    const share = await aliceOpake([
      "share",
      "new",
      "cancel-test.txt",
      "dave.test",
    ]);
    expect(share.code).toBe(0);
    expect(share.stdout).toContain("queued");

    // List to get the URI
    const pending = await aliceOpake(["share", "pending"]);
    expect(pending.code).toBe(0);
    const uri = pending.stdout.match(/at:\/\/[^\s)]+pendingShare[^\s)]+/)?.[0];
    expect(uri).toBeTruthy();

    // Cancel it
    const cancel = await aliceOpake(["share", "cancel", uri!]);
    expect(cancel.code).toBe(0);
    expect(cancel.stdout).toContain("cancelled");

    // Verify it's gone
    const afterCancel = await aliceOpake(["share", "pending"]);
    expect(afterCancel.stdout).not.toContain("dave.test");
  });
});
