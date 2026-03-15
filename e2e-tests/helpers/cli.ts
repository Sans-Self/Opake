// CLI subprocess runner — shells out to the compiled opake binary.

import { execFile } from "node:child_process";
import { resolve } from "node:path";

const BINARY = resolve(import.meta.dirname, "../../target/debug/opake");

export interface CliResult {
  readonly code: number;
  readonly stdout: string;
  readonly stderr: string;
}

/** Run an opake CLI command and capture output. */
export function opake(
  args: readonly string[],
  opts: {
    configDir: string;
    env?: Record<string, string>;
  },
): Promise<CliResult> {
  return new Promise((resolve) => {
    execFile(
      BINARY,
      ["--config-dir", opts.configDir, ...args],
      {
        env: { ...process.env, ...opts.env },
        timeout: 15_000,
      },
      (error, stdout, stderr) => {
        resolve({
          code: (typeof error?.code === "number" ? error.code : 0) as number,
          stdout: stdout.toString(),
          stderr: stderr.toString(),
        });
      },
    );
  });
}
