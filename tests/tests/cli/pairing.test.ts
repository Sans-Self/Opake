// Device pairing: pair request + pair approve
//
// Tests the full identity transfer flow between two "devices" (separate
// config directories) against the same fake PDS.

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { writeFileSync, readFileSync, mkdtempSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { spawn } from "node:child_process";
import { startPds, stopPds, resetPds, getPds } from "../../helpers/pds.js";
import { setupAccount } from "../../helpers/account.js";
import { opake } from "../../helpers/cli.js";

const BINARY = resolve(import.meta.dirname, "../../../target/debug/opake");

const tempDirs: string[] = [];

function freshDir(): string {
  const dir = mkdtempSync(join(tmpdir(), "opake-e2e-pair-"));
  tempDirs.push(dir);
  return dir;
}

/** Wait for a condition to become true, polling at interval. */
function waitFor(
  check: () => Promise<boolean>,
  interval: number,
  timeout: number,
): Promise<void> {
  return new Promise((resolve, reject) => {
    const deadline = Date.now() + timeout;
    const poll = async () => {
      if (await check()) {
        resolve();
        return;
      }
      if (Date.now() > deadline) {
        reject(new Error("waitFor timed out"));
        return;
      }
      setTimeout(() => void poll(), interval);
    };
    void poll();
  });
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

describe("device pairing", () => {
  // spec:auth-pairing § Pairing wraps the full identity to a device-held ephemeral keypair
  it("pair request → approve → new device can decrypt", async () => {
    resetPds();
    const pds = getPds();

    // --- Device A: existing device with full identity ---
    const deviceA = await setupAccount("did:plc:alice", "alice.test");
    tempDirs.push(deviceA.configDir);

    // Upload a file on Device A
    const workDir = freshDir();
    const testFile = join(workDir, "paired-file.txt");
    writeFileSync(testFile, "pairing test content");
    const upload = await opake(["upload", testFile], { configDir: deviceA.configDir });
    expect(upload.code).toBe(0);

    // --- Device B: logged in but NO local identity ---
    const deviceB = await setupAccount("did:plc:alice", "alice.test", { skipIdentity: true });
    tempDirs.push(deviceB.configDir);

    // --- Start pair request on Device B (background, polls every 1s) ---
    const pairRequestProcess = spawn(
      BINARY,
      ["--config-dir", deviceB.configDir, "pair", "request", "--interval", "1"],
      {
        env: { ...process.env },
        stdio: ["pipe", "pipe", "pipe"],
        timeout: 20_000,
      },
    );

    const requestStdout: Buffer[] = [];
    const requestStderr: Buffer[] = [];
    pairRequestProcess.stdout.on("data", (chunk: Buffer) => requestStdout.push(chunk));
    pairRequestProcess.stderr.on("data", (chunk: Buffer) => requestStderr.push(chunk));

    // Wait for the pair request record to appear on the PDS
    await waitFor(
      async () => {
        const records = pds.listRecords("did:plc:alice", "at.opake.pairRequest");
        return records.length > 0;
      },
      200,
      10_000,
    );

    // --- Device A: approve the pairing request ---
    // pair approve lists requests and prompts "Approve which request? [1-N]"
    const approve = await opake(["pair", "approve"], {
      configDir: deviceA.configDir,
      stdin: "1\n",
    });
    expect(approve.code).toBe(0);
    expect(approve.stdout).toContain("Identity sent");

    // --- Wait for Device B to receive the identity and exit ---
    const pairResult = await new Promise<{ code: number; stdout: string; stderr: string }>(
      (resolve) => {
        pairRequestProcess.on("close", (code) => {
          resolve({
            code: code ?? 1,
            stdout: Buffer.concat(requestStdout).toString(),
            stderr: Buffer.concat(requestStderr).toString(),
          });
        });
      },
    );

    expect(pairResult.code).toBe(0);
    expect(pairResult.stdout).toContain("Identity received");
    expect(pairResult.stdout).toContain("Pairing complete");

    // --- Verify Device B can decrypt files uploaded by Device A ---
    const downloadPath = join(workDir, "paired-download.txt");
    const download = await opake(
      ["download", "paired-file.txt", "-o", downloadPath],
      { configDir: deviceB.configDir },
    );
    expect(download.code).toBe(0);
    expect(readFileSync(downloadPath, "utf-8")).toBe("pairing test content");
  });

  // spec:auth-pairing § A device that already holds an identity refuses to request pairing
  it("pair request fails when identity already exists", async () => {
    resetPds();

    // Device with full identity
    const device = await setupAccount("did:plc:alice", "alice.test");
    tempDirs.push(device.configDir);

    const result = await opake(["pair", "request"], { configDir: device.configDir });
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("already has");
  });
});
