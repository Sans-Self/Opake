// Global setup for Playwright web e2e tests.
//
// Starts a fake-pds instance (OAuth mode) and a Vite dev server pointing at it.
// Registers a pool of test accounts for parallel isolation.

import { createFakePds, type FakePds } from "fake-pds";
import { spawn, type ChildProcess } from "node:child_process";
import { writeFileSync, unlinkSync, mkdirSync } from "node:fs";
import path from "node:path";
import { generateAccounts, AccountPool } from "fake-pds";

const STATE_FILE = path.join(import.meta.dirname, ".e2e-state.json");


function waitForViteReady(proc: ChildProcess, timeoutMs = 30_000): Promise<string> {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error(`Vite dev server did not start within ${timeoutMs}ms`));
    }, timeoutMs);

    const onData = (chunk: Buffer) => {
      const text = chunk.toString();
      const match = /https?:\/\/localhost:\d+/.exec(text);
      if (match) {
        clearTimeout(timeout);
        proc.stdout?.off("data", onData);
        resolve(match[0]);
      }
    };

    proc.stdout?.on("data", onData);
    proc.stderr?.on("data", (chunk: Buffer) => {
      process.stderr.write(chunk);
    });

    proc.on("error", (err) => {
      clearTimeout(timeout);
      reject(err);
    });

    proc.on("exit", (code) => {
      clearTimeout(timeout);
      if (code !== null && code !== 0) {
        reject(new Error(`Vite dev server exited with code ${code}`));
      }
    });
  });
}

export default async function globalSetup(): Promise<() => Promise<void>> {
  // 1. Generate account pool and start fake-pds
  const pool = generateAccounts();
  const pds: FakePds = await createFakePds({
    accounts: [...pool],
    auth: "oauth",
  });

  // 2. Start Vite dev server with env vars pointing at fake-pds
  const webRoot = path.resolve(import.meta.dirname, "../web");
  const vite: ChildProcess = spawn("bun", ["run", "dev"], {
    cwd: webRoot,
    env: {
      ...process.env,
      VITE_RESOLVE_API: pds.url,
      VITE_PLC_DIRECTORY_URL: pds.url,
      VITE_INDEXER_URL: "",
    },
    stdio: ["pipe", "pipe", "pipe"],
  });

  const webUrl = await waitForViteReady(vite);

  // 3. Write state + pool for test fixtures
  mkdirSync(path.dirname(STATE_FILE), { recursive: true });
  writeFileSync(
    STATE_FILE,
    JSON.stringify({ pdsUrl: pds.url, webUrl, pool }),
  );

  // 4. Clean stale lock files from prior runs
  const accountPool = new AccountPool(pool);
  accountPool.clearLocks();

  // 5. Teardown
  return async () => {
    vite.kill("SIGTERM");
    await pds.close();
    accountPool.clearLocks();
    try {
      unlinkSync(STATE_FILE);
    } catch {
      // already cleaned up
    }
  };
}
