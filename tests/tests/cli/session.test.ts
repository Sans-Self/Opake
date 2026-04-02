// Account session management and daemon commands

import { describe, it, expect } from "vitest";
import { readFileSync, existsSync, mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawn } from "node:child_process";
import { resolve } from "node:path";
import { useFixture } from "../../helpers/fixture.js";

const fx = useFixture();

const BINARY = resolve(import.meta.dirname, "../../../target/debug/opake");

describe("opake account session refresh", () => {
  it("refreshes a legacy session", async () => {
    const result = await fx.opake(["account", "session", "refresh"]);
    // Legacy sessions always trigger a refresh (no expires_at)
    expect(result.code).toBe(0);
    expect(result.stdout).toContain("Session refreshed");
    expect(result.stdout).toContain("alice.test");
  });

  it("persists the refreshed session to disk", async () => {
    const sessionPath = join(
      fx.ctx.configDir,
      "accounts",
      "did_plc_alice",
      "session.json",
    );
    const before = JSON.parse(readFileSync(sessionPath, "utf-8"));

    const result = await fx.opake(["account", "session", "refresh"]);
    expect(result.code).toBe(0);

    const after = JSON.parse(readFileSync(sessionPath, "utf-8"));
    // Legacy refresh returns new JWTs — at least one token should differ
    expect(after.accessJwt).not.toBe(before.accessJwt);
  });

  it("accepts --threshold flag", async () => {
    const result = await fx.opake([
      "account",
      "session",
      "refresh",
      "--threshold",
      "600",
    ]);
    expect(result.code).toBe(0);
    expect(result.stdout).toContain("alice.test");
  });
});

describe("opake daemon install", () => {
  it("generates a launchd plist on macOS", async () => {
    if (process.platform !== "darwin") return;

    // Isolate: override $HOME so we don't write to the real LaunchAgents
    const fakeHome = mkdtempSync(join(tmpdir(), "opake-home-"));
    const plistPath = join(
      fakeHome,
      "Library",
      "LaunchAgents",
      "app.opake.daemon.plist",
    );

    const result = await fx.opake(["daemon", "install"], { HOME: fakeHome });
    expect(result.code).toBe(0);
    expect(result.stdout).toContain("Service file written");
    expect(result.stdout).toContain("launchctl load");
    expect(existsSync(plistPath)).toBe(true);

    const plist = readFileSync(plistPath, "utf-8");
    expect(plist).toContain("app.opake.daemon");
    expect(plist).toContain("daemon");
    expect(plist).toContain("run");
  });

  it("generates a systemd unit on Linux", async () => {
    if (process.platform !== "linux") return;

    const fakeHome = mkdtempSync(join(tmpdir(), "opake-home-"));
    const unitPath = join(
      fakeHome,
      ".config",
      "systemd",
      "user",
      "opake-daemon.service",
    );

    const result = await fx.opake(["daemon", "install"], { HOME: fakeHome });
    expect(result.code).toBe(0);
    expect(result.stdout).toContain("Service file written");
    expect(result.stdout).toContain("systemctl");
    expect(existsSync(unitPath)).toBe(true);

    const unit = readFileSync(unitPath, "utf-8");
    expect(unit).toContain("opake daemon run");
    expect(unit).toContain("[Service]");
  });
});

describe("opake daemon uninstall", () => {
  it("removes the service file", async () => {
    if (process.platform !== "darwin" && process.platform !== "linux") return;

    const fakeHome = mkdtempSync(join(tmpdir(), "opake-home-"));
    // Install first to have something to uninstall
    await fx.opake(["daemon", "install"], { HOME: fakeHome });

    const result = await fx.opake(["daemon", "uninstall"], { HOME: fakeHome });
    expect(result.code).toBe(0);
    expect(result.stdout).toContain("Removed");
  });

  it("handles missing service file gracefully", async () => {
    const fakeHome = mkdtempSync(join(tmpdir(), "opake-home-"));
    const result = await fx.opake(["daemon", "uninstall"], {
      HOME: fakeHome,
    });
    expect(result.code).toBe(0);
    expect(result.stdout).toContain("No service file found");
  });
});

describe("opake daemon run", () => {
  it("refreshes sessions and exits on SIGINT", async () => {
    const child = spawn(
      BINARY,
      [
        "--config-dir",
        fx.ctx.configDir,
        "daemon",
        "run",
        "--threshold",
        "99999",
      ],
      {
        env: { ...process.env },
        stdio: ["pipe", "pipe", "pipe"],
      },
    );

    const stdoutChunks: Buffer[] = [];
    const stderrChunks: Buffer[] = [];

    child.stderr.on("data", (chunk: Buffer) => stderrChunks.push(chunk));

    // Wait for the daemon to print its startup message
    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(
        () => reject(new Error("daemon startup timeout")),
        10_000,
      );
      child.stdout.on("data", (chunk: Buffer) => {
        stdoutChunks.push(chunk);
        if (
          Buffer.concat(stdoutChunks).toString().includes("daemon starting")
        ) {
          clearTimeout(timeout);
          resolve();
        }
      });
    });

    // Wait one tick for the refresh to happen
    await new Promise((r) => setTimeout(r, 1500));

    // Send SIGINT for graceful shutdown
    child.kill("SIGINT");

    const exitCode = await new Promise<number>((resolve) => {
      child.on("close", (code) => resolve(code ?? 1));
    });

    const stdout = Buffer.concat(stdoutChunks).toString();

    expect(stdout).toContain("opake daemon starting");
    expect(stdout).toContain("shutting down");
    expect(exitCode).toBe(0);
  });
});
