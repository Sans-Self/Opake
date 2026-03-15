// §6 from AGENT-BLACKBOX-TEST.md: Keyrings

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
  const dir = mkdtempSync(join(tmpdir(), "opake-e2e-kr-"));
  tempDirs.push(dir);
  return dir;
}

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

describe("keyrings", () => {
  // eslint-disable-next-line functional/no-let
  let krDocUri: string;

  it("create a keyring", async () => {
    const result = await aliceOpake(["keyring", "create", "family-photos"]);
    expect(result.code).toBe(0);
    expect(result.stdout).toContain("family-photos");
  });

  it("list keyrings", async () => {
    const ls = await aliceOpake(["keyring", "ls"]);
    expect(ls.code).toBe(0);
    expect(ls.stdout).toContain("family-photos");
    expect(ls.stdout).toContain("1 member");
  });

  it("upload under keyring", async () => {
    const workDir = freshDir();
    const testFile = join(workDir, "photo.txt");
    writeFileSync(testFile, "family photo metadata");

    const upload = await aliceOpake(["upload", testFile, "--keyring", "family-photos"]);
    expect(upload.code).toBe(0);

    const uriMatch = upload.stdout.match(/at:\/\/[^\s]+/);
    expect(uriMatch).toBeTruthy();
    krDocUri = uriMatch![0]!;
  });

  it("download own keyring-encrypted file", async () => {
    const workDir = freshDir();
    const downloadPath = join(workDir, "photo-download.txt");

    const download = await aliceOpake(["download", "photo.txt", "-o", downloadPath]);
    expect(download.code).toBe(0);
    expect(readFileSync(downloadPath, "utf-8")).toBe("family photo metadata");
  });

  it("add member", async () => {
    const add = await aliceOpake(["keyring", "add-member", "family-photos", "bob.test"]);
    expect(add.code).toBe(0);
    expect(add.stdout).toContain("bob");

    const ls = await aliceOpake(["keyring", "ls"]);
    expect(ls.stdout).toContain("2 member");
  });

  it("member downloads keyring-encrypted file", async () => {
    const workDir = freshDir();
    const downloadPath = join(workDir, "bob-kr-download.txt");

    const download = await bobOpake([
      "download", "--keyring-member", krDocUri, "-o", downloadPath,
    ]);
    if (download.code !== 0) console.error("BOB DOWNLOAD:", download.stderr);
    expect(download.code).toBe(0);
    expect(readFileSync(downloadPath, "utf-8")).toBe("family photo metadata");
  });

  it("remove member rotates key", async () => {
    const remove = await aliceOpake([
      "keyring", "remove-member", "family-photos", "bob.test", "-y",
    ]);
    expect(remove.code).toBe(0);
    expect(remove.stdout.toLowerCase()).toContain("removed");

    const ls = await aliceOpake(["keyring", "ls"]);
    expect(ls.stdout).toContain("1 member");
  });

  it("removed member cannot download new uploads", async () => {
    const workDir = freshDir();

    // Alice uploads a new file under the rotated keyring
    const testFile = join(workDir, "post-rotation.txt");
    writeFileSync(testFile, "after bob was removed");
    const upload = await aliceOpake(["upload", testFile, "--keyring", "family-photos"]);
    expect(upload.code).toBe(0);

    const newUri = upload.stdout.match(/at:\/\/[^\s]+/)?.[0];
    expect(newUri).toBeTruthy();

    // Bob tries to download the new file — should fail
    const downloadPath = join(workDir, "bob-post-rotation.txt");
    const download = await bobOpake([
      "download", "--keyring-member", newUri!, "-o", downloadPath,
    ]);
    expect(download.code).not.toBe(0);
  });

  it("upload under nonexistent keyring fails", async () => {
    const workDir = freshDir();
    const testFile = join(workDir, "orphan-kr.txt");
    writeFileSync(testFile, "no keyring");
    const upload = await aliceOpake(["upload", testFile, "--keyring", "no-such-keyring"]);
    expect(upload.code).not.toBe(0);
  });

  it("non-member download fails", async () => {
    // Bob was removed earlier — try downloading the original file
    const workDir = freshDir();
    const downloadPath = join(workDir, "non-member.txt");
    const download = await bobOpake([
      "download", "--keyring-member", krDocUri, "-o", downloadPath,
    ]);
    expect(download.code).not.toBe(0);
  });
});
