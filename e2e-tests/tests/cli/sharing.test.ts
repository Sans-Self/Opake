// §5 from AGENT-BLACKBOX-TEST.md: Sharing (Grants)

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { writeFileSync, readFileSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { startPds, stopPds, getPds } from "../../helpers/pds.js";
import { setupAccount } from "../../helpers/account.js";
import { opake } from "../../helpers/cli.js";

// eslint-disable-next-line functional/no-let
let alice: Awaited<ReturnType<typeof setupAccount>>;
// eslint-disable-next-line functional/no-let
let bob: Awaited<ReturnType<typeof setupAccount>>;

const tempDirs: string[] = [];

function freshDir(): string {
  const dir = mkdtempSync(join(tmpdir(), "opake-e2e-share-"));
  tempDirs.push(dir);
  return dir;
}

/** Run opake with PLC directory pointed at fake-pds. */
function aliceOpake(args: readonly string[]) {
  return opake(args, {
    configDir: alice.configDir,
    env: { OPAKE_PLC_DIRECTORY: getPds().url },
  });
}

function bobOpake(args: readonly string[]) {
  return opake(args, {
    configDir: bob.configDir,
    env: { OPAKE_PLC_DIRECTORY: getPds().url },
  });
}

beforeAll(async () => {
  await startPds();
  alice = await setupAccount("did:plc:alice", "alice.test");
  bob = await setupAccount("did:plc:bob", "bob.test");
});

afterAll(async () => {
  await stopPds();
  for (const dir of tempDirs) {
    rmSync(dir, { recursive: true, force: true });
  }
  rmSync(alice.configDir, { recursive: true, force: true });
  rmSync(bob.configDir, { recursive: true, force: true });
});

describe("sharing", () => {
  it("alice shares with bob, bob downloads via grant", async () => {
    const workDir = freshDir();

    // Alice uploads a file
    const testFile = join(workDir, "shared-secret.txt");
    writeFileSync(testFile, "shared secret content");
    const upload = await aliceOpake(["upload", testFile]);
    expect(upload.code).toBe(0);

    // Alice shares with bob (resolves bob's identity via PLC directory)
    const share = await aliceOpake(["share", "new", "shared-secret.txt", "bob.test"]);
    if (share.code !== 0) console.error("SHARE:", share.stderr);
    expect(share.code).toBe(0);
    expect(share.stdout).toContain("bob");

    // Extract grant URI
    const grantUri = share.stdout.match(/at:\/\/[^\s]+/)?.[0];
    expect(grantUri).toBeTruthy();

    // Bob downloads via grant
    const downloadPath = join(workDir, "bob-download.txt");
    const download = await bobOpake(["download", "--grant", grantUri!, "-o", downloadPath]);
    if (download.code !== 0) console.error("DOWNLOAD:", download.stderr);
    expect(download.code).toBe(0);
    expect(readFileSync(downloadPath, "utf-8")).toBe("shared secret content");
  });

  it("alice revokes grant, bob download fails", async () => {
    const workDir = freshDir();

    const testFile = join(workDir, "revoke-test.txt");
    writeFileSync(testFile, "will be revoked");
    await aliceOpake(["upload", testFile]);

    const share = await aliceOpake(["share", "new", "revoke-test.txt", "bob.test"]);
    const grantUri = share.stdout.match(/at:\/\/[^\s]+/)?.[0];
    expect(grantUri).toBeTruthy();

    // Revoke
    const revoke = await aliceOpake(["share", "revoke", grantUri!, "-y"]);
    expect(revoke.code).toBe(0);

    // Bob's download should fail
    const downloadPath = join(workDir, "revoked-download.txt");
    const download = await bobOpake(["download", "--grant", grantUri!, "-o", downloadPath]);
    expect(download.code).not.toBe(0);
  });

  it("resolve command works with PLC directory override", async () => {
    const resolve = await aliceOpake(["resolve", "bob.test"]);
    expect(resolve.code).toBe(0);
    expect(resolve.stdout).toContain("did:plc:bob");
    expect(resolve.stdout).toContain("x25519");
  });
});
