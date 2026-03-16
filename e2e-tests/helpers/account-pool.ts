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

const LOCKS_DIR = path.join(import.meta.dirname, "../.e2e-locks");

// Dutch-flavored names for the Bakker household test universe
const NAMES = [
  "anika", "bram", "carmen", "daan", "eline", "floris", "greta", "hugo",
  "iris", "jesse", "karin", "lieke", "mees", "noor", "otto", "pien",
  "quinn", "ruben", "sanne", "thijs", "ulla", "vera", "wouter", "xander",
  "yara", "zev", "anke", "bas", "cato", "dirk", "eva", "fem",
  "guus", "hanna", "ivo", "jip", "kees", "lotte", "max", "niek",
  "olga", "pepijn", "roos", "stef", "tessa", "udo", "vince", "wies",
  "xenia", "yves",
] as const;

export const POOL_SIZE = NAMES.length;

/** Generate the full pool of test accounts for global-setup registration. */
export function generatePool(): readonly TestAccount[] {
  return NAMES.map((name) => ({
    handle: `${name}.test`,
    did: `did:plc:${name}`,
  }));
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
