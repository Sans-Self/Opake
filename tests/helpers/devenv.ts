// CLI driver for the dockerized dev-env (dev-env/). The opake CLI is a native
// Rust binary that resolves DIDs and fetches records over the network; under
// the dev-env's egress blockade the PDSes, PLC, and indexer are internal-only
// (only Caddy is host-published, on 127.0.0.1:443), and the host does not
// resolve *.test. Rather than require /etc/hosts edits or a --resolve-style
// override the binary does not support, we run the CLI *inside* the compose
// network — exactly as bootstrap.sh and verify-cli.sh do — where the service
// names (plc, indexer, pds-a) and Caddy's per-PDS aliases resolve natively.
//
// A single long-lived container is started once per suite (`docker compose run
// -d`), and every CLI invocation is a `docker exec` into it so account state
// (config, session, identity, cached group keys) persists across calls. The
// container's environment is set explicitly at creation, so the ambient shell
// env — the repo .envrc exports OPAKE_INDEXER_URL / VITE_INDEXER_URL for
// host-side dev — cannot leak in and point the CLI at a non-hermetic indexer
// (the same shadowing the web harness had to defend against).

import { execFile } from "node:child_process";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { devenvActors, type DevenvActor } from "./pds.js";

const run = promisify(execFile);

const COMPOSE_FILE = fileURLToPath(
  new URL("../../dev-env/docker-compose.yml", import.meta.url),
);
// Distinct from the compose-managed service containers; this is the suite's
// own ephemeral CLI runner, created and destroyed by the harness. Vitest runs
// test files in parallel worker processes, so the name is per-process — two
// federation files each spinning up a runner must not collide on one name (a
// shared name races the docker daemon into an "RWLayer unexpectedly nil"
// half-created container). Each worker owns its container and its own login
// state; the dev-env tolerates concurrent sessions for a fixture actor.
const CLI_CONTAINER = `opake-fed-cli-${process.pid}`;

// Inside the network the CLI reaches services by their compose names. The PDS
// URL per account is written into config.toml (http, internal), so DID-doc
// resolution never has to run for the caller's own PDS.
const IN_NETWORK_ENV = {
  OPAKE_PLC_DIRECTORY: "http://plc:2582",
  OPAKE_INDEXER_URL: "http://indexer:6100",
  // Foreign-PDS record fetches (cross-PDS pubkey resolution by DID) go over
  // the Caddy https endpoints; rustls honors the dev CA via SSL_CERT_FILE.
  SSL_CERT_FILE: "/certs/ca.crt",
} as const;

export interface CliResult {
  readonly code: number;
  readonly stdout: string;
  readonly stderr: string;
}

async function docker(args: readonly string[], timeoutMs = 90_000): Promise<CliResult> {
  try {
    const { stdout, stderr } = await run("docker", [...args], {
      timeout: timeoutMs,
      maxBuffer: 8 * 1024 * 1024,
    });
    return { code: 0, stdout, stderr };
  } catch (err) {
    const e = err as { code?: number; stdout?: string; stderr?: string; message?: string };
    return {
      code: typeof e.code === "number" ? e.code : 1,
      stdout: e.stdout ?? "",
      stderr: e.stderr ?? e.message ?? "",
    };
  }
}

/** True iff the dev-env indexer service is running (the suite's up-front gate). */
export async function stackIsUp(): Promise<boolean> {
  const res = await docker(
    ["compose", "-f", COMPOSE_FILE, "ps", "-q", "indexer"],
    15_000,
  );
  return res.code === 0 && res.stdout.trim() !== "";
}

/**
 * Fetch the indexer's public health snapshot from inside the compose network
 * (the dev-env indexer is internal-only; only Caddy is host-published). Used
 * best-effort by the pipeline preflight to name cursor/lag evidence on a
 * stalled-pipeline failure — the [ConsumeLag] log line is referenced, not
 * parsed. Returns a compact one-line summary, or a reason it was unavailable;
 * requires the shared CLI container (`startCli`) to be running.
 */
export async function indexerHealth(): Promise<string> {
  const res = await docker(
    ["exec", CLI_CONTAINER, "curl", "-fsS", "http://indexer:6100/api/health"],
    15_000,
  );
  if (res.code !== 0) return `unavailable (${res.stderr.trim() || "curl failed"})`;
  try {
    const h = JSON.parse(res.stdout) as {
      indexer_connected?: boolean;
      cursor_time?: string | null;
      cursor_age_secs?: number | null;
      events?: { last_event_age_ms?: number | null };
    };
    return (
      `indexer_connected=${h.indexer_connected} ` +
      `cursor_time=${h.cursor_time ?? "none"} ` +
      `cursor_age_secs=${h.cursor_age_secs ?? "none"} ` +
      `last_event_age_ms=${h.events?.last_event_age_ms ?? "none"}`
    );
  } catch {
    return res.stdout.trim().slice(0, 300);
  }
}

// The in-container helper (login + keyring delete) is a real shell file rather
// than an inlined string — bash parameter expansion (${did//:/_}) collides with
// JS template interpolation, and a standalone script stays readable and lintable.
const HELPER_PATH = fileURLToPath(new URL("./devenv-cli.sh", import.meta.url));
const HELPER_IN_CONTAINER = "/work-helper.sh";

/** Start the shared CLI container and install the helper script. Idempotent. */
export async function startCli(): Promise<void> {
  await docker(["rm", "-f", CLI_CONTAINER], 30_000);
  const envFlags = Object.entries(IN_NETWORK_ENV).flatMap(([k, v]) => [
    "-e",
    `${k}=${v}`,
  ]);
  const up = await docker([
    "compose",
    "-f",
    COMPOSE_FILE,
    "run",
    "-d",
    "--name",
    CLI_CONTAINER,
    ...envFlags,
    "bootstrap",
    "-c",
    "sleep infinity",
  ]);
  if (up.code !== 0) {
    throw new Error(`failed to start CLI container: ${up.stderr}`);
  }
  const install = await docker([
    "cp",
    HELPER_PATH,
    `${CLI_CONTAINER}:${HELPER_IN_CONTAINER}`,
  ]);
  if (install.code !== 0) {
    throw new Error(`failed to install helper: ${install.stderr}`);
  }
}

/** Tear the shared CLI container down. Safe to call if it never started. */
export async function stopCli(): Promise<void> {
  await docker(["rm", "-f", CLI_CONTAINER], 30_000);
}

/**
 * Log a fixture actor in inside the container; returns their DID. The actor's
 * credentials travel with the call (ACTOR_JSON) rather than being looked up in
 * the container's checked-in fixtures file: under an actor namespace the handle
 * and mnemonic are derived, and only the host knows them.
 */
export async function login(actor: string): Promise<string> {
  const res = await docker(
    [
      "exec",
      "-e",
      `ACTOR_JSON=${JSON.stringify(actorByName(actor))}`,
      CLI_CONTAINER,
      "bash",
      "/work-helper.sh",
      "login",
      actor,
    ],
    60_000,
  );
  if (res.code !== 0) throw new Error(`login ${actor} failed: ${res.stderr}`);
  return res.stdout.trim();
}

/** Run an opake CLI command as the given actor (account state persists). */
export async function cli(actor: string, args: readonly string[]): Promise<CliResult> {
  return docker([
    "exec",
    CLI_CONTAINER,
    "env",
    `OPAKE_DATA_DIR=/work/${actor}`,
    "opake",
    ...args,
  ]);
}

/** Delete a keyring record by rkey over XRPC, authored by the actor's session. */
export async function deleteKeyringRecord(actor: string, rkey: string): Promise<void> {
  const res = await docker(
    ["exec", CLI_CONTAINER, "bash", "/work-helper.sh", "delkr", actor, rkey],
    30_000,
  );
  if (res.code !== 0) throw new Error(`delkr ${rkey} failed: ${res.stderr}`);
}

/**
 * Stash then delete an actor's `publicKey/self` record so a share to them
 * hits `RecipientNotReady`. The dev-env bootstrap seeds every actor with a
 * published key, so this is the only way to reach the not-ready branch; the
 * record is preserved for `restorePublicKey` rather than regenerated.
 */
export async function unpublishPublicKey(actor: string): Promise<void> {
  const res = await docker(
    ["exec", CLI_CONTAINER, "bash", "/work-helper.sh", "delpubkey", actor],
    30_000,
  );
  if (res.code !== 0) throw new Error(`delpubkey ${actor} failed: ${res.stderr}`);
}

/** Restore the `publicKey/self` record stashed by `unpublishPublicKey`. */
export async function restorePublicKey(actor: string): Promise<void> {
  const res = await docker(
    ["exec", CLI_CONTAINER, "bash", "/work-helper.sh", "putpubkey", actor],
    30_000,
  );
  if (res.code !== 0) throw new Error(`putpubkey ${actor} failed: ${res.stderr}`);
}

/**
 * Rewrite a record's `createdAt` in place, preserving every other field
 * (including real encrypted payloads). Ages a genuine pending share past its
 * 7-day TTL without a real wait and without hand-forging a record.
 */
export async function backdateRecord(
  actor: string,
  collection: string,
  rkey: string,
  createdAt: string,
): Promise<void> {
  const res = await docker(
    ["exec", CLI_CONTAINER, "bash", "/work-helper.sh", "backdate", actor, collection, rkey, createdAt],
    30_000,
  );
  if (res.code !== 0) throw new Error(`backdate ${collection}/${rkey} failed: ${res.stderr}`);
}

/**
 * Encrypt-and-upload a small text document into the actor's personal cabinet;
 * returns its AT-URI. The CLI genesis-creates the cabinet root on first write,
 * so no prior seeding is required. Mirrors `uploadTextToWorkspace` without the
 * `--workspace` flag.
 */
export async function uploadTextToCabinet(
  actor: string,
  filename: string,
  content: string,
): Promise<string> {
  const path = `/tmp/${actor}-${filename}`;
  const b64 = Buffer.from(content, "utf8").toString("base64");
  const write = await docker([
    "exec",
    CLI_CONTAINER,
    "bash",
    "-c",
    `echo ${b64} | base64 -d > ${path}`,
  ]);
  if (write.code !== 0) throw new Error(`stage ${path} failed: ${write.stderr}`);
  const res = await cli(actor, ["upload", path]);
  if (res.code !== 0) throw new Error(`cabinet upload failed: ${res.stderr}`);
  return parseCreateUri(res.stdout);
}

/** Parse the at-uri a `workspace create` prints ("name → at://…/rkey"). */
export function parseCreateUri(stdout: string): string {
  const m = stdout.match(/at:\/\/[^\s]+/);
  if (!m) throw new Error(`no at-uri in create output: ${stdout}`);
  return m[0];
}

export const rkeyOf = (uri: string): string => uri.slice(uri.lastIndexOf("/") + 1);

/**
 * Poll until `predicate` holds or the deadline passes. The dev-env pipeline
 * (PDS → relay → jetstream → indexer) delivers writes in ~0.3–2s, so every
 * indexer-observed assertion waits it out rather than sleeping a fixed guess.
 */
export async function pollUntil(
  predicate: () => Promise<boolean>,
  { timeoutMs = 30_000, intervalMs = 1_000 }: { timeoutMs?: number; intervalMs?: number } = {},
): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    if (await predicate()) return true;
    if (Date.now() >= deadline) return false;
    await new Promise((r) => setTimeout(r, intervalMs));
  }
}

/** True once the named workspace appears in the actor's indexer-backed list. */
export async function workspaceListed(actor: string, name: string): Promise<boolean> {
  const res = await cli(actor, ["workspace", "ls", "-l"]);
  return res.code === 0 && res.stdout.includes(name);
}

/** Member count the indexer reports for a workspace in the actor's `ls -l`. */
export async function memberCount(actor: string, name: string): Promise<number | null> {
  const res = await cli(actor, ["workspace", "ls", "-l"]);
  if (res.code !== 0) return null;
  const line = res.stdout.split("\n").find((l) => l.includes(name));
  const m = line?.match(/(\d+) member\(s\)/);
  return m ? Number(m[1]) : null;
}

// `workspace ls -l` prints the chain HEAD URI (its authority is the DID hosting
// the current head), not the genesis URI — see the `ls` command's own comment.
// A supersede mints a new head record, so this value churns on every membership
// change while the genesis URI (captured at `create`) stays fixed. The churn
// tests read it to prove the head moved past genesis and kept moving, precisely
// the condition under which a head-keyed call site would 403.

/** The chain-head URI the indexer reports for a workspace in `ls -l`, or null. */
export async function headUri(actor: string, name: string): Promise<string | null> {
  const res = await cli(actor, ["workspace", "ls", "-l"]);
  if (res.code !== 0) return null;
  const line = res.stdout.split("\n").find((l) => l.includes(name));
  // `[^\s]+` stops at the tab before the `(member)` role tag, so this is the
  // bare URI even when the actor is a non-hosting member.
  const m = line?.match(/at:\/\/[^\s]+/);
  return m ? m[0] : null;
}

/** Group-key rotation counter the indexer reports for a workspace, or null. */
export async function rotationCount(actor: string, name: string): Promise<number | null> {
  const res = await cli(actor, ["workspace", "ls", "-l"]);
  if (res.code !== 0) return null;
  const line = res.stdout.split("\n").find((l) => l.includes(name));
  const m = line?.match(/rotation:(\d+)/);
  return m ? Number(m[1]) : null;
}

/**
 * Encrypt-and-upload a small text document into a workspace; returns its AT-URI.
 * The plaintext is staged inside the container via base64 (no shell-quoting
 * hazard) before the CLI reads and encrypts it client-side.
 */
export async function uploadTextToWorkspace(
  actor: string,
  workspace: string,
  filename: string,
  content: string,
): Promise<string> {
  const path = `/tmp/${actor}-${filename}`;
  const b64 = Buffer.from(content, "utf8").toString("base64");
  const write = await docker([
    "exec",
    CLI_CONTAINER,
    "bash",
    "-c",
    `echo ${b64} | base64 -d > ${path}`,
  ]);
  if (write.code !== 0) throw new Error(`stage ${path} failed: ${write.stderr}`);
  const res = await cli(actor, ["upload", path, "--workspace", workspace]);
  if (res.code !== 0) throw new Error(`upload to ${workspace} failed: ${res.stderr}`);
  return parseCreateUri(res.stdout);
}

/** First-time cross-PDS workspace-member download, printing plaintext to stdout. */
export async function downloadAsMember(actor: string, docUri: string): Promise<CliResult> {
  return cli(actor, ["download", "--workspace-member", docUri, "--stdout"]);
}

/** Resolve a fixture actor by name (for actors not identified by PDS slot). */
export function actorByName(name: string): DevenvActor {
  const actor = devenvActors().find((a) => a.name === name);
  if (!actor) throw new Error(`no fixture actor named ${name}`);
  return actor;
}

export function actorOnPds(pds: string): DevenvActor {
  const actor = devenvActors().find((a) => a.pds === pds);
  if (!actor) throw new Error(`no fixture actor on ${pds}`);
  return actor;
}
