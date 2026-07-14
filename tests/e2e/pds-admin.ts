// Host-side PDS record surgery for the web e2e tier — test-support only, never
// product code. The Playwright browser reaches the dev-env PDSes through Caddy
// via `--host-resolver-rules` (fixtures/playwright.config), but the Node test
// process has no such mapping, so it talks to Caddy directly on 127.0.0.1:443
// with the target PDS as the TLS servername + Host header, trusting the dev CA.
//
// Its one job: make a fully-seeded fixture actor "not ready" (delete their
// published `publicKey/self`) so a share to them hits RecipientNotReady — the
// warn-before-queue branch is otherwise unreachable, because the dev-env
// bootstrap publishes a key for every actor. The stashed record is restored on
// teardown so the shared actor is left exactly as found.
import { execFile } from "node:child_process";
import https from "node:https";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
// Extension-explicit: this module is imported from the vitest/tsc side too
// (federation tier), where Node16 resolution requires it.
import { actorsFor, assertNamespace, type Actor } from "./namespace.js";

const PUBLIC_KEY_COLLECTION = "app.opake.publicKey";
const PUBLIC_KEY_RKEY = "self";

// The dev-env's PDS admin password (dev-env/docker-compose.yml). A public test
// credential for a network with no egress — not a secret, and pinned here so
// the harness does not depend on the ambient shell exporting it.
const ADMIN_PASSWORD = process.env.PDS_ADMIN_PASSWORD ?? "101642ca520d8df36b77a979";

const run = promisify(execFile);

interface XrpcResponse {
  readonly status: number;
  readonly json: unknown;
}

/** The PDS's Caddy vhost for an actor, e.g. actor.pds "pds-c" → "pds-c.test". */
const pdsHost = (actor: Actor): string => `${actor.pds}.test`;

function xrpc(
  host: string,
  path: string,
  opts: { method?: string; token?: string; admin?: boolean; body?: unknown } = {},
): Promise<XrpcResponse> {
  const { method = "GET", token, admin, body } = opts;
  const payload = body === undefined ? null : JSON.stringify(body);
  const headers: Record<string, string> = { host, "content-type": "application/json" };
  if (token) headers.authorization = `Bearer ${token}`;
  if (admin) {
    const basic = Buffer.from(`admin:${ADMIN_PASSWORD}`).toString("base64");
    headers.authorization = `Basic ${basic}`;
  }
  if (payload) headers["content-length"] = String(Buffer.byteLength(payload));

  return new Promise((resolve, reject) => {
    const req = https.request(
      {
        host: "127.0.0.1",
        port: 443,
        path,
        method,
        servername: host, // SNI so Caddy routes to the right PDS vhost
        headers,
        rejectUnauthorized: false, // dev CA
      },
      (res) => {
        // eslint-disable-next-line functional/no-let
        let raw = "";
        res.on("data", (chunk) => (raw += chunk));
        res.on("end", () => {
          const json = raw ? JSON.parse(raw) : null;
          resolve({ status: res.statusCode ?? 0, json });
        });
      },
    );
    req.on("error", reject);
    if (payload) req.write(payload);
    req.end();
  });
}

const rkeyOf = (uri: string): string => uri.slice(uri.lastIndexOf("/") + 1);

/** Resolve an actor's DID via a session (handy for matching indexer output). */
export async function getDid(actor: Actor): Promise<string> {
  return (await createSession(actor)).did;
}

/**
 * Delete every `app.opake.grant` the sharer holds naming `recipient` — test
 * isolation for the sharing specs, whose sharer accumulates outgoing grants
 * across runs. Only touches grants between these two test actors.
 */
export async function clearGrantsTo(sharer: Actor, recipient: Actor): Promise<void> {
  const host = pdsHost(sharer);
  const { did, token } = await createSession(sharer);
  const recipientDid = await getDid(recipient);
  const list = await xrpc(
    host,
    `/xrpc/com.atproto.repo.listRecords?repo=${did}&collection=app.opake.grant&limit=100`,
  );
  if (list.status !== 200) return;
  const records = (list.json as { records?: { uri: string; value: { recipient?: string } }[] }).records ?? [];
  for (const rec of records) {
    if (rec.value.recipient !== recipientDid) continue;
    await xrpc(host, "/xrpc/com.atproto.repo.deleteRecord", {
      method: "POST",
      token,
      body: { repo: did, collection: "app.opake.grant", rkey: rkeyOf(rec.uri) },
    });
  }
}

async function createSession(actor: Actor): Promise<{ did: string; token: string }> {
  const host = pdsHost(actor);
  const res = await xrpc(host, "/xrpc/com.atproto.server.createSession", {
    method: "POST",
    body: { identifier: actor.handle, password: actor.password },
  });
  if (res.status !== 200) {
    throw new Error(`createSession ${actor.handle} failed: ${res.status} ${JSON.stringify(res.json)}`);
  }
  const s = res.json as { did: string; accessJwt: string };
  return { did: s.did, token: s.accessJwt };
}

/**
 * Delete an actor's published encryption key so a share to them resolves as
 * RecipientNotReady. Returns a restore function that re-publishes the exact
 * record that was removed — call it in a `finally` so the shared fixture actor
 * is left ready for other specs.
 */
export async function unpublishPublicKey(actor: Actor): Promise<() => Promise<void>> {
  const host = pdsHost(actor);
  const { did, token } = await createSession(actor);

  const existing = await xrpc(
    host,
    `/xrpc/com.atproto.repo.getRecord?repo=${did}&collection=${PUBLIC_KEY_COLLECTION}&rkey=${PUBLIC_KEY_RKEY}`,
  );
  if (existing.status !== 200) {
    throw new Error(`getRecord publicKey/self for ${actor.handle} failed: ${existing.status}`);
  }
  const record = (existing.json as { value: unknown }).value;

  const del = await xrpc(host, "/xrpc/com.atproto.repo.deleteRecord", {
    method: "POST",
    token,
    body: { repo: did, collection: PUBLIC_KEY_COLLECTION, rkey: PUBLIC_KEY_RKEY },
  });
  if (del.status !== 200) {
    throw new Error(`deleteRecord publicKey/self for ${actor.handle} failed: ${del.status}`);
  }

  return async () => {
    await xrpc(host, "/xrpc/com.atproto.repo.putRecord", {
      method: "POST",
      token,
      body: { repo: did, collection: PUBLIC_KEY_COLLECTION, rkey: PUBLIC_KEY_RKEY, record },
    });
  };
}

// ---------------------------------------------------------------------------
// Namespace lifecycle
// ---------------------------------------------------------------------------

const COMPOSE_FILE = fileURLToPath(
  new URL("../../dev-env/docker-compose.yml", import.meta.url),
);

/** The DID behind a handle, or null if the PDS does not know it. */
export async function resolveHandle(actor: Actor): Promise<string | null> {
  const res = await xrpc(
    pdsHost(actor),
    `/xrpc/com.atproto.identity.resolveHandle?handle=${encodeURIComponent(actor.handle)}`,
  );
  if (res.status !== 200) return null;
  return (res.json as { did?: string }).did ?? null;
}

/** An actor's published encryption key record, or null if they have none. */
export async function publishedPublicKey(actor: Actor): Promise<unknown | null> {
  const did = await resolveHandle(actor);
  if (!did) return null;
  const res = await xrpc(
    pdsHost(actor),
    `/xrpc/com.atproto.repo.getRecord?repo=${did}&collection=${PUBLIC_KEY_COLLECTION}&rkey=${PUBLIC_KEY_RKEY}`,
  );
  if (res.status !== 200) return null;
  return (res.json as { value: unknown }).value;
}

export interface PublishedKeys {
  readonly x25519: string;
  readonly mlKem: string;
  readonly signing: string;
}

/**
 * The key material an actor has published, with the record's `createdAt` left
 * out: two provisionings of the same actor mint records at different instants,
 * but a deterministic identity means the KEYS are byte-identical. That is the
 * equality the determinism guarantee is about.
 */
export async function publishedEncryptionKeys(actor: Actor): Promise<PublishedKeys | null> {
  const value = (await publishedPublicKey(actor)) as {
    x25519PublicKey?: { $bytes?: string };
    mlKemPublicKey?: { $bytes?: string };
    signingKey?: { $bytes?: string };
  } | null;
  if (!value?.x25519PublicKey?.$bytes) return null;
  return {
    x25519: value.x25519PublicKey.$bytes,
    mlKem: value.mlKemPublicKey?.$bytes ?? "",
    signing: value.signingKey?.$bytes ?? "",
  };
}

/**
 * Provision the actors of `ns` that do not exist yet, and return their names.
 *
 * Idempotent by handle resolution: an actor whose handle already resolves is
 * left exactly as it is (its records, workspaces, and sessions included), so a
 * second run of a namespace costs one resolve per actor and nothing else.
 *
 * The work itself runs INSIDE the compose network via the dev-env's own
 * bootstrap recipe, fed a namespace-scoped fixtures document over the
 * environment (nothing is written to disk, no manifest exists). Two reasons it
 * is not re-implemented over host XRPC: the identity import and the
 * `publicKey/self` publication need opake's key derivation (PBKDF2 → HKDF →
 * X25519 + ML-KEM), which a TypeScript harness cannot reproduce without forking
 * the crypto; and running the same recipe is what makes "same guarantees as a
 * bootstrapped actor" a fact rather than an aspiration — live account on the
 * role's PDS, published key derived from the mnemonic, seeded cabinet root
 * (except `frank`, deliberately left rootless, as in the default population).
 */
export async function provisionNamespace(ns: string): Promise<readonly string[]> {
  assertNamespace(ns);
  const actors = actorsFor(ns);
  const live = await Promise.all(actors.map(resolveHandle));
  const missing = actors.filter((_, i) => live[i] === null);
  if (missing.length === 0) return [];

  const fixtures = {
    actors: missing.map((actor) => ({
      name: actor.name,
      handle: actor.handle,
      pds: actor.pds,
      mnemonic: actor.mnemonic,
      password: actor.password,
      // Namespaced actors share a PDS with the default population, whose emails
      // are derived from the bare role name — collide on that and createAccount
      // rejects the registration.
      email: `${actor.name}-${ns}@${actor.pds}.test`,
    })),
  };

  const script = [
    "set -euo pipefail",
    'printf "%s" "$NS_ACTORS" > /tmp/ns-actors.json',
    "FIXTURES=/tmp/ns-actors.json /bootstrap/bootstrap.sh",
  ].join("\n");

  await run(
    "docker",
    [
      "compose",
      "-f",
      COMPOSE_FILE,
      "run",
      "--rm",
      "-e",
      `NS_ACTORS=${JSON.stringify(fixtures)}`,
      "bootstrap",
      "-c",
      script,
    ],
    { timeout: 600_000, maxBuffer: 8 * 1024 * 1024 },
  );

  return missing.map((actor) => actor.name);
}

/**
 * Delete every account of `ns` — records and blobs with them — addressed purely
 * by derived handle, so no state file has to survive between provisioning and
 * teardown. Refuses the default population: those six are checked in, and the
 * only sanctioned way to clear them is a full dev-env reset.
 */
export async function deprovisionNamespace(ns: string): Promise<readonly string[]> {
  if (ns.trim() === "") {
    throw new Error(
      "refusing to deprovision the default actor namespace — it is the checked-in " +
        "fixture set; use `just dev-env-reset` to clear the whole environment",
    );
  }
  assertNamespace(ns);

  const actors = actorsFor(ns);
  const dids = await Promise.all(actors.map(resolveHandle));
  const live = actors
    .map((actor, i) => ({ actor, did: dids[i] }))
    .filter((t): t is { actor: Actor; did: string } => t.did !== null);

  await Promise.all(
    live.map(async ({ actor, did }) => {
      const res = await xrpc(pdsHost(actor), "/xrpc/com.atproto.admin.deleteAccount", {
        method: "POST",
        admin: true,
        body: { did },
      });
      if (res.status !== 200) {
        throw new Error(
          `deleteAccount ${actor.handle} (${did}) failed: ${res.status} ${JSON.stringify(res.json)}`,
        );
      }
    }),
  );
  return live.map(({ actor }) => actor.name);
}
