// Daemon maintenance tasks: pair request cleanup.
//
// Grant healing e2e is deferred — it requires cross-PDS DID resolution
// which the fake-pds doesn't fully support for external DIDs. The heal
// logic has unit test coverage via MockTransport.

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { spawn } from "node:child_process";
import { startPds, stopPds, getPds } from "../../helpers/pds.js";
import { setupAccount } from "../../helpers/account.js";

const BINARY = resolve(import.meta.dirname, "../../../target/debug/opake");
const tempDirs: string[] = [];

beforeAll(async () => {
  await startPds();
});

afterAll(async () => {
  await stopPds();
  for (const dir of tempDirs) {
    rmSync(dir, { recursive: true, force: true });
  }
});

/** Run the daemon briefly and return its stderr (log output). */
async function runDaemonOnce(
  configDir: string,
  env?: Record<string, string>,
): Promise<string> {
  const child = spawn(
    BINARY,
    ["--config-dir", configDir, "-v", "daemon", "run", "--threshold", "0"],
    { env: { ...process.env, ...env }, stdio: ["pipe", "pipe", "pipe"] },
  );

  const stdoutChunks: Buffer[] = [];
  const stderrChunks: Buffer[] = [];
  child.stdout.on("data", (chunk: Buffer) => stdoutChunks.push(chunk));
  child.stderr.on("data", (chunk: Buffer) => stderrChunks.push(chunk));

  // Wait for startup
  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(
      () => reject(new Error("daemon startup timeout")),
      10_000,
    );
    child.stdout.on("data", () => {
      if (Buffer.concat(stdoutChunks).toString().includes("daemon starting")) {
        clearTimeout(timeout);
        resolve();
      }
    });
  });

  // Let all first-tick tasks fire
  await new Promise((r) => setTimeout(r, 3000));
  child.kill("SIGINT");
  await new Promise<void>((resolve) => child.on("close", () => resolve()));

  return Buffer.concat(stderrChunks).toString();
}

describe("pair request cleanup", () => {
  it("deletes expired pair requests", async () => {
    const pds = getPds();
    const ctx = await setupAccount("did:plc:alice", "alice.test");
    tempDirs.push(ctx.configDir);

    // Seed an expired pair request directly on the fake PDS (20 min old)
    const twentyMinutesAgo = new Date(Date.now() - 20 * 60 * 1000).toISOString();
    pds.putRecord("did:plc:alice", "at.opake.pairRequest", "expired1", {
      $type: "at.opake.pairRequest",
      opakeVersion: 1,
      ephemeralKey: { $bytes: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" },
      algo: "x25519",
      createdAt: twentyMinutesAgo,
    });

    const before = pds.listRecords("did:plc:alice", "at.opake.pairRequest");
    expect(before.length).toBeGreaterThanOrEqual(1);

    await runDaemonOnce(ctx.configDir);

    const after = pds.listRecords("did:plc:alice", "at.opake.pairRequest");
    const expired = after.filter(
      (r: { uri: string }) => r.uri.includes("expired1"),
    );
    expect(expired.length).toBe(0);
  });

  it("keeps fresh pair requests", async () => {
    const pds = getPds();
    const ctx = await setupAccount("did:plc:alice", "alice.test");
    tempDirs.push(ctx.configDir);

    // Seed a fresh pair request (just now)
    pds.putRecord("did:plc:alice", "at.opake.pairRequest", "fresh1", {
      $type: "at.opake.pairRequest",
      opakeVersion: 1,
      ephemeralKey: { $bytes: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" },
      algo: "x25519",
      createdAt: new Date().toISOString(),
    });

    const before = pds.listRecords("did:plc:alice", "at.opake.pairRequest");
    expect(before.length).toBeGreaterThanOrEqual(1);

    await runDaemonOnce(ctx.configDir);

    const after = pds.listRecords("did:plc:alice", "at.opake.pairRequest");
    const fresh = after.filter(
      (r: { uri: string }) => r.uri.includes("fresh1"),
    );
    expect(fresh.length).toBe(1);
  });

  it("deletes orphaned pair responses", async () => {
    const pds = getPds();
    const ctx = await setupAccount("did:plc:alice", "alice.test");
    tempDirs.push(ctx.configDir);

    // Seed an orphaned pair response (no matching request)
    pds.putRecord("did:plc:alice", "at.opake.pairResponse", "orphan1", {
      $type: "at.opake.pairResponse",
      opakeVersion: 1,
      request: "at://did:plc:alice/at.opake.pairRequest/doesnotexist",
      wrappedKey: {
        did: "did:plc:alice",
        ciphertext: { $bytes: "AAAA" },
        algo: "x25519-mlkem768-hkdf-a256kw-v2",
      },
      ciphertext: { $bytes: "BBBB" },
      nonce: { $bytes: "CCCC" },
      algo: "aes-256-gcm",
      createdAt: new Date().toISOString(),
    });

    const before = pds.listRecords("did:plc:alice", "at.opake.pairResponse");
    expect(before.length).toBeGreaterThanOrEqual(1);

    await runDaemonOnce(ctx.configDir);

    const after = pds.listRecords("did:plc:alice", "at.opake.pairResponse");
    const orphans = after.filter(
      (r: { uri: string }) => r.uri.includes("orphan1"),
    );
    expect(orphans.length).toBe(0);
  });
});

describe("stale grant healing", () => {
  it("deletes grant when recipient DID is unresolvable", async () => {
    const pds = getPds();
    const ctx = await setupAccount("did:plc:alice", "alice.test");
    tempDirs.push(ctx.configDir);

    // Seed a grant with a recipient DID that exists in the fake-pds
    // but has NO publicKey/self record (simulates deactivated account)
    pds.putRecord("did:plc:alice", "at.opake.grant", "stale1", {
      $type: "at.opake.grant",
      opakeVersion: 1,
      document: "at://did:plc:alice/at.opake.document/fakedoc",
      recipient: "did:plc:bob",
      wrappedKey: {
        did: "did:plc:bob",
        ciphertext: { $bytes: "AAAA" },
        algo: "x25519-mlkem768-hkdf-a256kw-v2",
      },
      encryptedMetadata: {
        ciphertext: { $bytes: "BBBB" },
        nonce: { $bytes: "CCCC" },
      },
      createdAt: new Date().toISOString(),
    });

    // Bob exists in fake-pds (DID resolves) but has no publicKey/self
    // The fake-pds already has bob registered but setupAccount wasn't called
    // for bob, so no publicKey/self record exists.

    const before = pds.listRecords("did:plc:alice", "at.opake.grant");
    expect(before.length).toBeGreaterThanOrEqual(1);

    // Point DID resolution at the fake-pds so did:plc:bob resolves locally
    await runDaemonOnce(ctx.configDir, {
      OPAKE_PLC_DIRECTORY: ctx.pdsUrl,
    });

    const after = pds.listRecords("did:plc:alice", "at.opake.grant");
    const staleGrants = after.filter((r: { uri: string }) =>
      r.uri.includes("stale1"),
    );
    expect(staleGrants.length).toBe(0);
  });

  it("keeps grant when recipient has a valid public key", async () => {
    const pds = getPds();
    const alice = await setupAccount("did:plc:alice", "alice.test");
    tempDirs.push(alice.configDir);

    // Set up bob WITH a public key
    const bob = await setupAccount("did:plc:bob", "bob.test");
    tempDirs.push(bob.configDir);

    // Seed a grant to bob (who has a valid publicKey/self)
    pds.putRecord("did:plc:alice", "at.opake.grant", "valid1", {
      $type: "at.opake.grant",
      opakeVersion: 1,
      document: "at://did:plc:alice/at.opake.document/fakedoc",
      recipient: "did:plc:bob",
      wrappedKey: {
        did: "did:plc:bob",
        ciphertext: { $bytes: "AAAA" },
        algo: "x25519-mlkem768-hkdf-a256kw-v2",
      },
      encryptedMetadata: {
        ciphertext: { $bytes: "BBBB" },
        nonce: { $bytes: "CCCC" },
      },
      createdAt: new Date().toISOString(),
    });

    const before = pds.listRecords("did:plc:alice", "at.opake.grant");
    const validBefore = before.filter((r: { uri: string }) =>
      r.uri.includes("valid1"),
    );
    expect(validBefore.length).toBe(1);

    await runDaemonOnce(alice.configDir, {
      OPAKE_PLC_DIRECTORY: alice.pdsUrl,
    });

    const after = pds.listRecords("did:plc:alice", "at.opake.grant");
    const validAfter = after.filter((r: { uri: string }) =>
      r.uri.includes("valid1"),
    );
    expect(validAfter.length).toBe(1);
  });
});
