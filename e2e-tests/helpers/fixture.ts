// Shared test fixture: PDS lifecycle, account setup, temp dir management.
//
// Usage in test files:
//   import { useFixture } from "../../helpers/fixture.js";
//   const fx = useFixture();
//   // fx.ctx — account context (configDir, did, handle, pdsUrl)
//   // fx.workDir — fresh temp dir per test
//   // fx.opake(args) — run CLI with the fixture's configDir

import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { beforeAll, afterAll, beforeEach } from "vitest";
import { startPds, stopPds } from "./pds.js";
import { setupAccount } from "./account.js";
import { opake, type CliResult } from "./cli.js";

interface Fixture {
  readonly ctx: Awaited<ReturnType<typeof setupAccount>>;
  readonly workDir: string;
  opake(args: readonly string[], env?: Record<string, string>): Promise<CliResult>;
}

/**
 * Set up a complete test fixture with PDS, account, and per-test work dirs.
 * Call once at the top of each test file. Returns a mutable object that
 * updates between tests (workDir changes in beforeEach).
 */
export function useFixture(
  did = "did:plc:alice",
  handle = "alice.test",
): Fixture {
  const tempDirs: string[] = [];
  const state = {
    ctx: null as Awaited<ReturnType<typeof setupAccount>> | null,
    workDir: "",
  };

  beforeAll(async () => {
    await startPds();
    state.ctx = await setupAccount(did, handle);
  });

  beforeEach(() => {
    const dir = mkdtempSync(join(tmpdir(), "opake-e2e-"));
    tempDirs.push(dir);
    state.workDir = dir;
  });

  afterAll(async () => {
    await stopPds();
    for (const dir of tempDirs) {
      rmSync(dir, { recursive: true, force: true });
    }
    if (state.ctx?.configDir) {
      rmSync(state.ctx.configDir, { recursive: true, force: true });
    }
  });

  return {
    get ctx() {
      if (!state.ctx) throw new Error("fixture not initialized — beforeAll hasn't run");
      return state.ctx;
    },
    get workDir() {
      return state.workDir;
    },
    opake(args: readonly string[], env?: Record<string, string>) {
      if (!state.ctx) throw new Error("fixture not initialized");
      return opake(args, { configDir: state.ctx.configDir, env });
    },
  };
}
