// Account recovery from seed phrase

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { writeFileSync, readFileSync, rmSync, existsSync, mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { startPds, stopPds, resetPds } from "../../helpers/pds.js";
import { opake } from "../../helpers/cli.js";
import { interactiveLogin } from "../../helpers/login.js";

const tempDirs: string[] = [];

function freshDir(): string {
  const dir = mkdtempSync(join(tmpdir(), "opake-e2e-recover-"));
  tempDirs.push(dir);
  return dir;
}

beforeAll(async () => {
  await startPds();
});

afterAll(async () => {
  await stopPds();
  for (const dir of tempDirs) {
    rmSync(dir, { recursive: true, force: true });
  }
});

describe("recover failures", () => {
  // spec:auth-identity § A mnemonic is rejected unless it is 24 valid checksummed words
  it("recover rejects invalid seed phrase", async () => {
    resetPds();
    const configDir = freshDir();
    const login = await interactiveLogin("alice.test", configDir);
    expect(login.code).toBe(0);

    rmSync(join(configDir, "accounts", "did_plc_alice", "identity.json"));

    const badFile = join(freshDir(), "bad-seed.txt");
    writeFileSync(badFile, "not a valid seed phrase at all");
    const recover = await opake(["recover", "-f", badFile], { configDir });
    expect(recover.code).not.toBe(0);
    expect(recover.stderr).toContain("invalid");
  });

  it("recover rejects wrong word count", async () => {
    resetPds();
    const configDir = freshDir();
    await interactiveLogin("bob.test", configDir);

    rmSync(join(configDir, "accounts", "did_plc_bob", "identity.json"));

    const shortFile = join(freshDir(), "short-seed.txt");
    writeFileSync(shortFile, "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art");
    const recover = await opake(["recover", "-f", shortFile], { configDir });
    expect(recover.code).not.toBe(0);
  });

  // spec:auth-identity § Recovery re-derives and cross-checks the published key
  it("recover rejects when identity already exists", async () => {
    resetPds();
    const configDir = freshDir();
    const login = await interactiveLogin("alice.test", configDir);
    expect(login.code).toBe(0);

    const seedFile = join(freshDir(), "existing-seed.txt");
    writeFileSync(seedFile, login.seedPhrase);
    const recover = await opake(["recover", "-f", seedFile], { configDir });
    expect(recover.code).not.toBe(0);
    expect(recover.stderr).toContain("already exists");
  });

  it("recover rejects nonexistent file", async () => {
    resetPds();
    const configDir = freshDir();
    await interactiveLogin("bob.test", configDir);

    rmSync(join(configDir, "accounts", "did_plc_bob", "identity.json"));

    const recover = await opake(["recover", "-f", "/nonexistent/path.txt"], { configDir });
    expect(recover.code).not.toBe(0);
  });
});

describe("recover", () => {
  it("recover from saved grid file (.txt backup)", async () => {
    resetPds();
    const configDir = freshDir();
    const workDir = freshDir();
    const savedGridPath = join(workDir, "grid-backup.txt");

    const login = await interactiveLogin("alice.test", configDir, savedGridPath);
    expect(login.code).toBe(0);

    const testFile = join(workDir, "grid-recovery-test.txt");
    writeFileSync(testFile, "recoverable via grid");
    const upload = await opake(["upload", testFile], { configDir });
    expect(upload.code).toBe(0);

    rmSync(join(configDir, "accounts", "did_plc_alice", "identity.json"));

    expect(existsSync(savedGridPath)).toBe(true);
    const gridContent = readFileSync(savedGridPath, "utf-8");
    expect(gridContent).toContain("1.");

    const recover = await opake(["recover", "-f", savedGridPath], { configDir });
    expect(recover.code).toBe(0);
    expect(recover.stdout).toContain("recovered");

    const downloadPath = join(workDir, "grid-recovered.txt");
    const download = await opake(["download", "grid-recovery-test.txt", "-o", downloadPath], { configDir });
    expect(download.code).toBe(0);
    expect(readFileSync(downloadPath, "utf-8")).toBe("recoverable via grid");
  });

  // spec:auth-identity § Recovery re-derives and cross-checks the published key
  it("recover from plain text seed phrase → decrypt works", async () => {
    resetPds();
    const configDir = freshDir();
    const workDir = freshDir();

    const login = await interactiveLogin("bob.test", configDir);
    expect(login.code).toBe(0);
    expect(login.seedPhrase.split(" ").length).toBe(24);

    const testFile = join(workDir, "pre-recovery.txt");
    writeFileSync(testFile, "before recovery");
    const upload = await opake(["upload", testFile], { configDir });
    expect(upload.code).toBe(0);

    rmSync(join(configDir, "accounts", "did_plc_bob", "identity.json"));

    const seedFile = join(workDir, "seed-for-recovery.txt");
    writeFileSync(seedFile, login.seedPhrase);
    const recover = await opake(["recover", "-f", seedFile], { configDir });
    expect(recover.code).toBe(0);
    expect(recover.stdout).toContain("recovered");

    const downloadPath = join(workDir, "post-recovery.txt");
    const download = await opake(["download", "pre-recovery.txt", "-o", downloadPath], { configDir });
    expect(download.code).toBe(0);
    expect(readFileSync(downloadPath, "utf-8")).toBe("before recovery");
  });
});
