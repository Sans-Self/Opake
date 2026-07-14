// Helpers for the harness meta-tier: drive whole Playwright runs as child
// processes and read what they did.
//
// A meta-test cannot assert on the setup project from inside it — the thing
// under test IS a run. So these spawn real runs, in real namespaces, and the
// assertions read their stdout and the artifacts they left behind.
import { spawn } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { nsPaths } from "../namespace";

const TESTS_DIR = fileURLToPath(new URL("../..", import.meta.url));

export interface RunResult {
  readonly code: number;
  readonly stdout: string;
  readonly stderr: string;
}

export interface RunOptions {
  /** Actor namespace to scope the run to. */
  readonly ns: string;
  /** Playwright project (`setup`, `e2e`). */
  readonly project: string;
  /** Restrict to matching test titles. */
  readonly grep?: string;
  /** Force re-authentication of every actor (E2E_REAUTH=1). */
  readonly reauth?: boolean;
}

/** Run Playwright in a child process and resolve when it exits. */
export function runPlaywright(opts: RunOptions): Promise<RunResult> {
  const args = ["playwright", "test", `--project=${opts.project}`];
  const argv = opts.grep ? [...args, "--grep", opts.grep] : args;

  return new Promise((resolve, reject) => {
    const child = spawn("bunx", argv, {
      cwd: TESTS_DIR,
      env: {
        ...process.env,
        E2E_ACTOR_NS: opts.ns,
        // The parent harness run must not leak its own forcing into the child.
        ...(opts.reauth ? { E2E_REAUTH: "1" } : { E2E_REAUTH: "" }),
      },
    });
    // eslint-disable-next-line functional/no-let
    let stdout = "";
    // eslint-disable-next-line functional/no-let
    let stderr = "";
    child.stdout.on("data", (chunk: Buffer) => (stdout += chunk.toString()));
    child.stderr.on("data", (chunk: Buffer) => (stderr += chunk.toString()));
    child.on("error", reject);
    child.on("close", (code) => resolve({ code: code ?? 1, stdout, stderr }));
  });
}

/** Where a namespace's snapshot for one actor lives. */
export const snapshotFile = (ns: string, actor: string): string =>
  join(nsPaths(ns).authDir, `${actor}.json`);

const namesFrom = (stdout: string, verb: string): readonly string[] =>
  [...stdout.matchAll(new RegExp(`\\[auth\\.setup\\] (\\w+): ${verb}`, "g"))]
    .map((m) => m[1]!)
    .sort();

/** Actors whose persisted session passed the liveness probe and was reused. */
export const reusedActors = (stdout: string): readonly string[] =>
  namesFrom(stdout, "reusing live snapshot");

/** Actors that setup logged in again (probe rejected, or forced). */
export const reauthenticatedActors = (stdout: string): readonly string[] =>
  namesFrom(stdout, "re-authenticating");

/**
 * Leave a snapshot holding credentials the PDS will not honour: both tokens
 * corrupted, and the expiry backdated so the app reaches for the refresh token
 * and is turned away. Corrupting the refresh token alone is not enough — the
 * access token from a real login stays valid for the best part of an hour, so
 * the session is genuinely still alive and a probe is right to say so.
 *
 * This is the shape of the failure the liveness probe exists for: a snapshot the
 * file system vouches for and the server rejects. It is reached by editing the
 * persisted session, never by lifting a live token out of it for a side-channel
 * call.
 */
export function invalidateSnapshotSession(file: string): void {
  const snapshot = JSON.parse(readFileSync(file, "utf8")) as {
    origins: {
      indexedDB?: {
        stores?: { name: string; records?: { value?: { value?: Record<string, unknown> } }[] }[];
      }[];
    }[];
  };

  const records = snapshot.origins
    .flatMap((origin) => origin.indexedDB ?? [])
    .flatMap((db) => db.stores ?? [])
    .filter((store) => store.name === "sessions")
    .flatMap((store) => store.records ?? []);
  if (records.length === 0) {
    throw new Error(`no persisted session in ${file} — nothing to invalidate`);
  }

  for (const record of records) {
    const session = record.value?.value;
    if (!session) continue;
    session.expires_at = Math.floor(Date.now() / 1000) - 3600;
    session.access_token = `invalidated-${session.access_token as string}`;
    session.refresh_token = `invalidated-${session.refresh_token as string}`;
  }

  writeFileSync(file, JSON.stringify(snapshot));
}
