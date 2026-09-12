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
import { actorNamespace } from "../e2e/namespace.js";

const run = promisify(execFile);

const COMPOSE_FILE = fileURLToPath(new URL("../../dev-env/docker-compose.yml", import.meta.url));
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

/**
 * The local PDS deliberately models owner confirmation as a mail-delivered
 * `plc_operation` token. Production-path tests cannot receive that mail, so
 * this fixture reads the token from the *disposable namespaced actor's* PDS
 * database. It never writes the database or bypasses the signer: the value is
 * still supplied to `com.atproto.identity.signPlcOperation`, which validates
 * and consumes it in the running PDS.
 *
 * This pins the query to the PDS 0.5.9 image used by dev-env. Keep it here,
 * rather than in product code, so an application can never obtain an owner
 * confirmation through this side channel.
 */
const READ_PLC_OPERATION_TOKEN = String.raw`
const Database = require("/app/node_modules/.pnpm/better-sqlite3@12.11.1/node_modules/better-sqlite3");
const handle = process.argv[1];
const after = Number(process.argv[2]);
(async () => {
  const response = await fetch(
    "http://127.0.0.1:3000/xrpc/com.atproto.identity.resolveHandle?handle=" + encodeURIComponent(handle)
  );
  if (!response.ok) throw new Error("could not resolve fixture handle");
  const { did } = await response.json();
  const db = new Database("/pds/account.sqlite", { readonly: true });
  const row = db.prepare(
    "select token, requestedAt from email_token where did = ? and purpose = 'plc_operation'"
  ).get(did);
  db.close();
  if (!row || Date.parse(row.requestedAt) < after) process.exit(2);
  process.stdout.write(JSON.stringify(row));
})().catch((error) => { console.error(String(error)); process.exit(1); });
`;

/** Read the active PLC operation chain from inside the hermetic compose network. */
const READ_PLC_LOG = String.raw`
const handle = process.argv[1];
(async () => {
  const resolved = await fetch(
    "http://127.0.0.1:3000/xrpc/com.atproto.identity.resolveHandle?handle=" + encodeURIComponent(handle)
  );
  if (!resolved.ok) throw new Error("could not resolve fixture handle");
  const { did } = await resolved.json();
  const response = await fetch("http://plc:2582/" + encodeURIComponent(did) + "/log");
    if (!response.ok) throw new Error("PLC log returned " + response.status);
    process.stdout.write(JSON.stringify(await response.json()));
})().catch((error) => { console.error(String(error)); process.exit(1); });
`;

const READ_PLC_DOCUMENT = String.raw`
const handle = process.argv[1];
(async () => {
  const resolved = await fetch(
    "http://127.0.0.1:3000/xrpc/com.atproto.identity.resolveHandle?handle=" + encodeURIComponent(handle)
  );
  if (!resolved.ok) throw new Error("could not resolve fixture handle");
  const { did } = await resolved.json();
  const response = await fetch("http://plc:2582/" + encodeURIComponent(did));
  if (!response.ok) throw new Error("PLC DID document returned " + response.status);
  process.stdout.write(JSON.stringify(await response.json()));
})().catch((error) => { console.error(String(error)); process.exit(1); });
`;

// Deliberately reports only aggregate booleans. The production-path proof
// needs to distinguish the standing browser authority from a per-operation
// identity grant without reading, serializing, or logging either credential.
const READ_OAUTH_SCOPE_SUMMARY = String.raw`
const Database = require("/app/node_modules/.pnpm/better-sqlite3@12.11.1/node_modules/better-sqlite3");
const { createHash } = require("crypto");
const handle = process.argv[1];
const standingRefreshFingerprint = process.argv[2] || "";
(async () => {
  const response = await fetch(
    "http://127.0.0.1:3000/xrpc/com.atproto.identity.resolveHandle?handle=" + encodeURIComponent(handle)
  );
  if (!response.ok) throw new Error("could not resolve fixture handle");
  const { did } = await response.json();
  const db = new Database("/pds/account.sqlite", { readonly: true });
  const rows = db.prepare("select scope, currentRefreshToken from token where did = ?").all(did);
  db.close();
  const scopes = rows.map((row) => typeof row.scope === "string" ? row.scope.split(/\s+/) : []);
  const standingIndex = rows.findIndex((row) =>
    typeof row.currentRefreshToken === "string" &&
    createHash("sha256").update(row.currentRefreshToken).digest("hex") === standingRefreshFingerprint
  );
  const standingScope = standingIndex < 0 ? [] : scopes[standingIndex];
  process.stdout.write(JSON.stringify({
    tokenCount: rows.length,
    hasIdentityScope: scopes.some((scope) => scope.includes("identity:*")),
    identityTokenCount: scopes.filter((scope) => scope.includes("identity:*")).length,
    standingTokenFound: standingIndex >= 0,
    standingHasIdentityScope: standingScope.includes("identity:*"),
  }));
})().catch((error) => { console.error(String(error)); process.exit(1); });
`;

function requireIsolatedActorNamespace(): void {
  if (actorNamespace() === "") {
    throw new Error(
      "PLC owner-confirmation fixtures require E2E_ACTOR_NS; refusing default fixture actors",
    );
  }
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

/** A confirmation code read from a disposable PDS fixture, never product state. */
export interface PlcOwnerConfirmation {
  readonly code: string;
  readonly requestedAt: string;
}

/**
 * Wait for the owner-confirmation token which a real PDS generated after the
 * caller requested a PLC signature. `notBeforeMs` prevents a prior attempt's
 * unconsumed token from being mistaken for the current operation.
 */
export async function awaitPlcOwnerConfirmation(
  actor: string,
  notBeforeMs: number,
  { timeoutMs = 20_000, intervalMs = 200 }: { timeoutMs?: number; intervalMs?: number } = {},
): Promise<PlcOwnerConfirmation> {
  requireIsolatedActorNamespace();
  const fixture = actorByName(actor);
  const deadline = Date.now() + timeoutMs;
  let lastFailure = "no plc_operation token was present";
  for (;;) {
    const res = await docker(
      [
        "compose",
        "-f",
        COMPOSE_FILE,
        "exec",
        "-T",
        fixture.pds,
        "node",
        "-e",
        READ_PLC_OPERATION_TOKEN,
        fixture.handle,
        String(notBeforeMs),
      ],
      15_000,
    );
    if (res.code === 0) {
      try {
        const row = JSON.parse(res.stdout) as { token?: unknown; requestedAt?: unknown };
        if (typeof row.token === "string" && typeof row.requestedAt === "string") {
          return { code: row.token, requestedAt: row.requestedAt };
        }
        lastFailure = "PDS token row was malformed";
      } catch {
        lastFailure = "PDS token fixture returned malformed JSON";
      }
    } else if (res.code !== 2) {
      lastFailure = res.stderr.trim() || "PDS token fixture failed";
    }
    if (Date.now() >= deadline) {
      throw new Error(`timed out waiting for ${actor}'s PLC owner confirmation: ${lastFailure}`);
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
}

/** Current PLC operation log, obtained from the local directory without host DNS. */
export async function plcOperationLog(actor: string): Promise<readonly Record<string, unknown>[]> {
  requireIsolatedActorNamespace();
  const fixture = actorByName(actor);
  const res = await docker(
    [
      "compose",
      "-f",
      COMPOSE_FILE,
      "exec",
      "-T",
      fixture.pds,
      "node",
      "-e",
      READ_PLC_LOG,
      fixture.handle,
    ],
    20_000,
  );
  if (res.code !== 0) throw new Error(`read PLC log for ${actor} failed: ${res.stderr}`);
  const log = JSON.parse(res.stdout) as unknown;
  if (!Array.isArray(log) || !log.every((entry) => entry && typeof entry === "object")) {
    throw new Error(`PLC log for ${actor} was malformed`);
  }
  return log as readonly Record<string, unknown>[];
}

/** Authoritative current DID document from the local PLC directory. */
export async function plcDidDocument(actor: string): Promise<Record<string, unknown>> {
  requireIsolatedActorNamespace();
  const fixture = actorByName(actor);
  const res = await docker(
    [
      "compose",
      "-f",
      COMPOSE_FILE,
      "exec",
      "-T",
      fixture.pds,
      "node",
      "-e",
      READ_PLC_DOCUMENT,
      fixture.handle,
    ],
    20_000,
  );
  if (res.code !== 0) throw new Error(`read PLC DID document for ${actor} failed: ${res.stderr}`);
  const document = JSON.parse(res.stdout) as unknown;
  if (!document || typeof document !== "object") {
    throw new Error(`PLC DID document for ${actor} was malformed`);
  }
  return document as Record<string, unknown>;
}

/** Aggregate-only view of PDS OAuth scope rows for a disposable actor. */
export async function pdsOAuthScopeSummary(
  actor: string,
  standingRefreshFingerprint: string,
): Promise<{
  readonly tokenCount: number;
  readonly hasIdentityScope: boolean;
  readonly identityTokenCount: number;
  readonly standingTokenFound: boolean;
  readonly standingHasIdentityScope: boolean;
}> {
  requireIsolatedActorNamespace();
  const fixture = actorByName(actor);
  const res = await docker(
    [
      "compose",
      "-f",
      COMPOSE_FILE,
      "exec",
      "-T",
      fixture.pds,
      "node",
      "-e",
      READ_OAUTH_SCOPE_SUMMARY,
      fixture.handle,
      standingRefreshFingerprint,
    ],
    20_000,
  );
  if (res.code !== 0)
    throw new Error(`read OAuth scope summary for ${actor} failed: ${res.stderr}`);
  const summary = JSON.parse(res.stdout) as {
    tokenCount?: unknown;
    hasIdentityScope?: unknown;
    identityTokenCount?: unknown;
    standingTokenFound?: unknown;
    standingHasIdentityScope?: unknown;
  };
  if (
    typeof summary.tokenCount !== "number" ||
    typeof summary.hasIdentityScope !== "boolean" ||
    typeof summary.identityTokenCount !== "number" ||
    typeof summary.standingTokenFound !== "boolean" ||
    typeof summary.standingHasIdentityScope !== "boolean"
  ) {
    throw new Error(`OAuth scope summary for ${actor} was malformed`);
  }
  return {
    tokenCount: summary.tokenCount,
    hasIdentityScope: summary.hasIdentityScope,
    identityTokenCount: summary.identityTokenCount,
    standingTokenFound: summary.standingTokenFound,
    standingHasIdentityScope: summary.standingHasIdentityScope,
  };
}

/** True iff the dev-env indexer service is running (the suite's up-front gate). */
export async function stackIsUp(): Promise<boolean> {
  const res = await docker(["compose", "-f", COMPOSE_FILE, "ps", "-q", "indexer"], 15_000);
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
const DROP_PROXY_CONTAINER = `opake-fed-drop-apply-${process.pid}`;
const DROP_PROXY_PORT = 3900 + (process.pid % 1000);
const HOLD_PROXY_CONTAINER = `opake-fed-hold-apply-${process.pid}`;
const HOLD_PROXY_PORT = 4900 + (process.pid % 1000);

// The proxy runs in the owner's PDS network namespace. It forwards a real
// applyWrites request to localhost:3000, then drops exactly that one response.
// This gives the production CLI an actual unknown-result path without changing
// its transport or the PDS: a later read must reconcile the durable pair.
const DROP_FIRST_APPLY_WRITES_PROXY = String.raw`
const http = require("http");
let dropNextApply = true;
const proxy = http.createServer((request, response) => {
  const upstream = http.request({
    host: "127.0.0.1", port: 3000, path: request.url, method: request.method,
    headers: request.headers,
  }, (upstreamResponse) => {
    const chunks = [];
    upstreamResponse.on("data", (chunk) => chunks.push(chunk));
    upstreamResponse.on("end", () => {
      const isApply = request.url.startsWith("/xrpc/com.atproto.repo.applyWrites");
      if (isApply && dropNextApply) {
        dropNextApply = false;
        request.socket.destroy();
        return;
      }
      response.writeHead(upstreamResponse.statusCode, upstreamResponse.headers);
      response.end(Buffer.concat(chunks));
    });
  });
  upstream.on("error", (error) => {
    response.writeHead(502, { "content-type": "text/plain" });
    response.end(String(error));
  });
  request.pipe(upstream);
});
proxy.listen(Number(process.env.PORT), "0.0.0.0");
`;

// The first applyWrites remains unforwarded until the test releases it. The
// held request proves its runner has already resolved recipient keys and built
// the real production write; a second runner can then resolve a replacement
// bundle and commit against the same pending intent.
const HOLD_FIRST_APPLY_WRITES_PROXY = String.raw`
const http = require("http");
let held = null;
let holdingFirstApply = true;
const forward = (request, response) => {
  const upstream = http.request({
    host: "127.0.0.1", port: 3000, path: request.url, method: request.method,
    headers: request.headers,
  }, (upstreamResponse) => {
    response.writeHead(upstreamResponse.statusCode, upstreamResponse.headers);
    upstreamResponse.pipe(response);
  });
  upstream.on("error", (error) => {
    response.writeHead(502, { "content-type": "text/plain" });
    response.end(String(error));
  });
  request.pipe(upstream);
};
const proxy = http.createServer((request, response) => {
  if (request.url === "/__opake_test_state") {
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ held: held !== null }));
    return;
  }
  if (request.url === "/__opake_test_release" && request.method === "POST") {
    if (held !== null) {
      const next = held;
      held = null;
      next.request.resume();
      forward(next.request, next.response);
    }
    response.writeHead(204);
    response.end();
    return;
  }
  const isApply = request.url.startsWith("/xrpc/com.atproto.repo.applyWrites");
  if (isApply && holdingFirstApply) {
    holdingFirstApply = false;
    request.pause();
    held = { request, response };
    return;
  }
  forward(request, response);
});
proxy.listen(Number(process.env.PORT), "0.0.0.0");
`;

/** Start the shared CLI container and install the helper script. Idempotent. */
export async function startCli(): Promise<void> {
  await docker(["rm", "-f", CLI_CONTAINER], 30_000);
  const envFlags = Object.entries(IN_NETWORK_ENV).flatMap(([k, v]) => ["-e", `${k}=${v}`]);
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
  const install = await docker(["cp", HELPER_PATH, `${CLI_CONTAINER}:${HELPER_IN_CONTAINER}`]);
  if (install.code !== 0) {
    throw new Error(`failed to install helper: ${install.stderr}`);
  }
}

/** Tear the shared CLI container down. Safe to call if it never started. */
export async function stopCli(): Promise<void> {
  await docker(["rm", "-f", CLI_CONTAINER], 30_000);
}

/**
 * Start a test-only proxy that drops the response after one real PDS
 * `applyWrites`. It attaches to the selected PDS's network namespace, so the
 * CLI can address it as `http://pds-a:<port>` and the proxy reaches the real
 * PDS through localhost. Call `stop` in a finally block.
 */
export async function startDropApplyWritesProxy(
  pds: string,
): Promise<{ readonly url: string; readonly stop: () => Promise<void> }> {
  const pdsContainer = await docker(["compose", "-f", COMPOSE_FILE, "ps", "-q", pds], 15_000);
  const target = pdsContainer.stdout.trim();
  if (pdsContainer.code !== 0 || target === "") {
    throw new Error(`could not find running ${pds} container: ${pdsContainer.stderr}`);
  }
  await docker(["rm", "-f", DROP_PROXY_CONTAINER], 15_000);
  const started = await docker([
    "run",
    "-d",
    "--rm",
    "--name",
    DROP_PROXY_CONTAINER,
    "--network",
    `container:${target}`,
    "-e",
    `PORT=${DROP_PROXY_PORT}`,
    "--entrypoint",
    "node",
    "ghcr.io/bluesky-social/pds:0.4",
    "-e",
    DROP_FIRST_APPLY_WRITES_PROXY,
  ]);
  if (started.code !== 0) {
    throw new Error(`failed to start applyWrites drop proxy: ${started.stderr}`);
  }
  return {
    url: `http://${pds}:${DROP_PROXY_PORT}`,
    stop: async () => {
      await docker(["rm", "-f", DROP_PROXY_CONTAINER], 15_000);
    },
  };
}

/**
 * Pause the first real `applyWrites` before it reaches the selected PDS.
 * This is test-only scheduling control for two production CLI runners; it
 * neither constructs nor submits a product write itself.
 */
export async function startHoldApplyWritesProxy(pds: string): Promise<{
  readonly url: string;
  readonly waitUntilHeld: () => Promise<void>;
  readonly release: () => Promise<void>;
  readonly stop: () => Promise<void>;
}> {
  const pdsContainer = await docker(["compose", "-f", COMPOSE_FILE, "ps", "-q", pds], 15_000);
  const target = pdsContainer.stdout.trim();
  if (pdsContainer.code !== 0 || target === "") {
    throw new Error(`could not find running ${pds} container: ${pdsContainer.stderr}`);
  }
  await docker(["rm", "-f", HOLD_PROXY_CONTAINER], 15_000);
  const started = await docker([
    "run",
    "-d",
    "--rm",
    "--name",
    HOLD_PROXY_CONTAINER,
    "--network",
    `container:${target}`,
    "-e",
    `PORT=${HOLD_PROXY_PORT}`,
    "--entrypoint",
    "node",
    "ghcr.io/bluesky-social/pds:0.4",
    "-e",
    HOLD_FIRST_APPLY_WRITES_PROXY,
  ]);
  if (started.code !== 0) {
    throw new Error(`failed to start applyWrites hold proxy: ${started.stderr}`);
  }
  const base = `http://${pds}:${HOLD_PROXY_PORT}`;
  const request = async (path: string, method = "GET"): Promise<CliResult> =>
    docker(["exec", CLI_CONTAINER, "curl", "-fsS", "-X", method, `${base}${path}`], 15_000);
  return {
    url: base,
    waitUntilHeld: async () => {
      const held = await pollUntil(
        async () => {
          const state = await request("/__opake_test_state");
          if (state.code !== 0) return false;
          try {
            return (JSON.parse(state.stdout) as { held?: unknown }).held === true;
          } catch {
            return false;
          }
        },
        { timeoutMs: 30_000, intervalMs: 100 },
      );
      if (!held) throw new Error("timed out waiting for the first applyWrites request to be held");
    },
    release: async () => {
      const released = await request("/__opake_test_release", "POST");
      if (released.code !== 0) {
        throw new Error(`failed to release held applyWrites request: ${released.stderr}`);
      }
    },
    stop: async () => {
      await docker(["rm", "-f", HOLD_PROXY_CONTAINER], 15_000);
    },
  };
}

/** Point one logged-in CLI account at a test proxy or restore its normal PDS URL. */
export async function setCliPdsUrl(actor: string, url: string): Promise<void> {
  const res = await docker(
    ["exec", CLI_CONTAINER, "bash", "/work-helper.sh", "setpds", actor, url],
    30_000,
  );
  if (res.code !== 0) throw new Error(`set PDS URL for ${actor} failed: ${res.stderr}`);
}

/** Clone one logged-in fixture account into an isolated local CLI data directory. */
export async function cloneCliAccount(source: string, target: string): Promise<void> {
  const res = await docker(
    ["exec", CLI_CONTAINER, "cp", "-a", `/work/${source}`, `/work/${target}`],
    30_000,
  );
  if (res.code !== 0) throw new Error(`clone CLI account ${source} failed: ${res.stderr}`);
}

/** Remove a test-owned cloned CLI data directory. */
export async function removeCliAccount(target: string): Promise<void> {
  const res = await docker(["exec", CLI_CONTAINER, "rm", "-rf", `/work/${target}`], 30_000);
  if (res.code !== 0) throw new Error(`remove cloned CLI account ${target} failed: ${res.stderr}`);
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
  return docker(["exec", CLI_CONTAINER, "env", `OPAKE_DATA_DIR=/work/${actor}`, "opake", ...args]);
}

/**
 * Run one operation with the CLI's explicit acknowledgement for the exact
 * unverified bundle it resolves during that operation. Keeping this at the
 * call site makes the consent visible in each production-path scenario;
 * it is not a harness-wide bypass.
 */
export async function cliApprovingUnverified(
  actor: string,
  args: readonly string[],
): Promise<CliResult> {
  return cli(actor, [...args, "--approve-unverified"]);
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
    [
      "exec",
      CLI_CONTAINER,
      "bash",
      "/work-helper.sh",
      "backdate",
      actor,
      collection,
      rkey,
      createdAt,
    ],
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
// the condition under which a head-keyed call site would resolve to no
// `chain_heads` row and be answered `workspace_not_indexed`.

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
