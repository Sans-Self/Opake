// CLI subprocess runner — shells out to the compiled opake binary.

import { spawn, execFile } from "node:child_process";
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
    stdin?: string;
  },
): Promise<CliResult> {
  if (opts.stdin !== undefined) {
    return opakeWithStdin(args, opts as typeof opts & { stdin: string });
  }

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

/**
 * Run opake with stdin piped. For interactive commands (login, rm without -y).
 * All stdin content is written at once and the stream is closed.
 */
function opakeWithStdin(
  args: readonly string[],
  opts: {
    configDir: string;
    env?: Record<string, string>;
    stdin: string;
  },
): Promise<CliResult> {
  return new Promise((resolve) => {
    const child = spawn(BINARY, ["--config-dir", opts.configDir, ...args], {
      env: { ...process.env, ...opts.env },
      stdio: ["pipe", "pipe", "pipe"],
      timeout: 15_000,
    });

    const stdoutChunks: Buffer[] = [];
    const stderrChunks: Buffer[] = [];

    child.stdout.on("data", (chunk: Buffer) => stdoutChunks.push(chunk));
    child.stderr.on("data", (chunk: Buffer) => stderrChunks.push(chunk));

    child.stdin.write(opts.stdin);
    child.stdin.end();

    child.on("close", (code) => {
      resolve({
        code: code ?? 1,
        stdout: Buffer.concat(stdoutChunks).toString(),
        stderr: Buffer.concat(stderrChunks).toString(),
      });
    });
  });
}

/**
 * Run opake with interactive stdin — responds to prompts line by line.
 * The responder function receives accumulated stdout+stderr and returns
 * the next line to send, or null to stop responding.
 */
export function opakeInteractive(
  args: readonly string[],
  opts: {
    configDir: string;
    env?: Record<string, string>;
    respond: (output: string) => string | null;
    responseDelay?: number;
  },
): Promise<CliResult> {
  return new Promise((resolve) => {
    const child = spawn(BINARY, ["--config-dir", opts.configDir, ...args], {
      env: { ...process.env, ...opts.env },
      stdio: ["pipe", "pipe", "pipe"],
      timeout: 30_000,
    });

    const stdoutChunks: Buffer[] = [];
    const stderrChunks: Buffer[] = [];
    const delay = opts.responseDelay ?? 50;

    function getOutput(): string {
      return (
        Buffer.concat(stdoutChunks).toString() +
        Buffer.concat(stderrChunks).toString()
      );
    }

    function tryRespond(): void {
      const output = getOutput();
      const response = opts.respond(output);
      if (response !== null) {
        child.stdin.write(response + "\n");
      }
    }

    child.stdout.on("data", (chunk: Buffer) => {
      stdoutChunks.push(chunk);
      setTimeout(tryRespond, delay);
    });

    child.stderr.on("data", (chunk: Buffer) => {
      stderrChunks.push(chunk);
      setTimeout(tryRespond, delay);
    });

    child.on("close", (code) => {
      resolve({
        code: code ?? 1,
        stdout: Buffer.concat(stdoutChunks).toString(),
        stderr: Buffer.concat(stderrChunks).toString(),
      });
    });
  });
}
