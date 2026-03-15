// §1 from AGENT-BLACKBOX-TEST.md: Login and Account Management

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { startPds, stopPds, getPds } from "../../helpers/pds.js";
import { opake } from "../../helpers/cli.js";
import { interactiveLogin } from "../../helpers/login.js";

const tempDirs: string[] = [];

function freshConfigDir(): string {
  const dir = mkdtempSync(join(tmpdir(), "opake-e2e-login-"));
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

describe("login", () => {
  it("legacy login with seed phrase confirmation", async () => {
    const configDir = freshConfigDir();
    const result = await interactiveLogin("alice.test", configDir);

    expect(result.code).toBe(0);
    expect(result.stdout).toContain("Logged in as");
    expect(result.stdout).toContain("Published encryption public key");
    expect(result.seedPhrase.split(" ").length).toBe(24);

    const accounts = await opake(["account", "list"], { configDir });
    expect(accounts.code).toBe(0);
    expect(accounts.stdout).toContain("alice.test");
  });

  it("second login adds account without overwriting first", async () => {
    const configDir = freshConfigDir();
    await interactiveLogin("alice.test", configDir);
    await interactiveLogin("bob.test", configDir);

    const accounts = await opake(["account", "list"], { configDir });
    expect(accounts.code).toBe(0);
    expect(accounts.stdout).toContain("alice.test");
    expect(accounts.stdout).toContain("bob.test");
  });
});

describe("account management", () => {
  it("set-default switches active account", async () => {
    const configDir = freshConfigDir();
    await interactiveLogin("alice.test", configDir);
    await interactiveLogin("bob.test", configDir);

    const setDefault = await opake(["account", "set-default", "bob.test"], { configDir });
    expect(setDefault.code).toBe(0);

    const accounts = await opake(["account", "list"], { configDir });
    expect(accounts.stdout).toContain("bob.test");
  });

  it("login with wrong password fails", async () => {
    const configDir = freshConfigDir();
    const pds = getPds();
    // Bob has password "secret" set in the accounts table
    // But we'll send "wrong" via env var
    const result = await opake(
      ["account", "login", "bob.test", "--legacy", "--pds", pds.url],
      { configDir, env: { OPAKE_CLI_PASSWORD: "wrong" } },
    );
    expect(result.code).not.toBe(0);
  });

  it("logout nonexistent account fails", async () => {
    const configDir = freshConfigDir();
    await interactiveLogin("alice.test", configDir);

    const logout = await opake(["account", "logout", "nobody.test"], { configDir });
    expect(logout.code).not.toBe(0);
  });

  it("set-default unknown handle fails", async () => {
    const configDir = freshConfigDir();
    await interactiveLogin("alice.test", configDir);

    const result = await opake(["account", "set-default", "nobody.test"], { configDir });
    expect(result.code).not.toBe(0);
  });

  it("logout removes an account", async () => {
    const configDir = freshConfigDir();
    await interactiveLogin("alice.test", configDir);

    const logout = await opake(["account", "logout", "alice.test"], { configDir });
    expect(logout.code).toBe(0);

    const accounts = await opake(["account", "list"], { configDir });
    expect(accounts.stdout).not.toContain("alice.test");
  });
});
