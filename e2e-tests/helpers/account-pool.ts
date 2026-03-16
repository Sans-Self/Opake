// Account pool for parallel test isolation.
//
// Each test acquires a unique account via atomic file locks, ensuring no two
// concurrent tests share PDS state. Accounts are pre-registered in global-setup.

import { writeFileSync, unlinkSync, mkdirSync } from "node:fs";
import path from "node:path";

export interface TestAccount {
  readonly handle: string;
  readonly did: string;
}

export const POOL_SIZE = 50;

const LOCKS_DIR = path.join(import.meta.dirname, "../.e2e-locks");

/** Generate the full pool of test accounts for global-setup registration. */
export function generatePool(): readonly TestAccount[] {
  return Array.from({ length: POOL_SIZE }, (_, i) => {
    const id = String(i).padStart(2, "0");
    return { handle: `test-${id}.test`, did: `did:plc:test-${id}` };
  });
}

/** Acquire an unused account from the pool (atomic file lock). */
export function acquireAccount(pool: readonly TestAccount[]): {
  account: TestAccount;
  release: () => void;
} {
  mkdirSync(LOCKS_DIR, { recursive: true });

  for (const account of pool) {
    const lockFile = path.join(LOCKS_DIR, account.did.replaceAll(":", "_"));
    try {
      // wx = exclusive create — fails if file already exists (atomic)
      writeFileSync(lockFile, process.pid.toString(), { flag: "wx" });
      return {
        account,
        release: () => {
          try {
            unlinkSync(lockFile);
          } catch {
            // already released
          }
        },
      };
    } catch {
      continue; // already locked by another worker
    }
  }

  throw new Error(`No available test accounts (pool size: ${pool.length})`);
}

/** Clean up all lock files (called in global teardown). */
export function clearAllLocks(): void {
  try {
    const { readdirSync } = require("node:fs") as typeof import("node:fs");
    for (const file of readdirSync(LOCKS_DIR)) {
      unlinkSync(path.join(LOCKS_DIR, file));
    }
    unlinkSync(LOCKS_DIR);
  } catch {
    // directory doesn't exist or already cleaned
  }
}
