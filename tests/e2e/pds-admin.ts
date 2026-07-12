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
import https from "node:https";
import type { Actor } from "./fixtures";

const PUBLIC_KEY_COLLECTION = "app.opake.publicKey";
const PUBLIC_KEY_RKEY = "self";

interface XrpcResponse {
  readonly status: number;
  readonly json: unknown;
}

/** The PDS's Caddy vhost for an actor, e.g. actor.pds "pds-c" → "pds-c.test". */
const pdsHost = (actor: Actor): string => `${actor.pds}.test`;

function xrpc(
  host: string,
  path: string,
  opts: { method?: string; token?: string; body?: unknown } = {},
): Promise<XrpcResponse> {
  const { method = "GET", token, body } = opts;
  const payload = body === undefined ? null : JSON.stringify(body);
  const headers: Record<string, string> = { host, "content-type": "application/json" };
  if (token) headers.authorization = `Bearer ${token}`;
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
