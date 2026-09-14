// Test-only OAuth primitives for real-PDS revocation tests. Private key
// material stays inside the returned closure and is never logged or exported.
import { randomUUID, webcrypto } from "node:crypto";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import https from "node:https";
import { dirname, resolve as resolvePath } from "node:path";
import { fileURLToPath } from "node:url";
import type { Page } from "@playwright/test";
import type { Actor } from "./namespace";
// Generated from `opake_core::scope` by `just ts-bindings`: the fixture must
// declare the same client metadata the product does, or a scope drift passes
// the test and fails in the browser.
import { CLIENT_METADATA_SCOPE } from "../../packages/opake-sdk/src/generated/OauthScope";

const encoder = new TextEncoder();

function base64url(bytes: ArrayBuffer | Uint8Array): string {
  return Buffer.from(bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes))
    .toString("base64")
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/u, "");
}

async function digest(value: string): Promise<Uint8Array> {
  return new Uint8Array(await webcrypto.subtle.digest("SHA-256", encoder.encode(value)));
}

export async function generatePkcePair(): Promise<{ readonly verifier: string; readonly challenge: string }> {
  const verifier = base64url(webcrypto.getRandomValues(new Uint8Array(32)));
  return { verifier, challenge: base64url(await digest(verifier)) };
}

export interface TestDpopKey {
  readonly publicJwk: JsonWebKey;
  proof(input: { readonly htu: string; readonly htm: string; readonly nonce?: string; readonly accessToken?: string }): Promise<string>;
  nativeFixtureKey(): { readonly private_key_b64: string; readonly public_jwk: { readonly kty: string; readonly crv: string; readonly x: string; readonly y: string } };
}

export async function generateTestDpopKey(): Promise<TestDpopKey> {
  const keys = await webcrypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign", "verify"],
  );
  const publicJwk = await webcrypto.subtle.exportKey("jwk", keys.publicKey);
  const privateJwk = await webcrypto.subtle.exportKey("jwk", keys.privateKey);
  if (!publicJwk.kty || !publicJwk.crv || !publicJwk.x || !publicJwk.y || !privateJwk.d) {
    throw new Error("test DPoP key export was incomplete");
  }
  return {
    publicJwk,
    async proof({ htu, htm, nonce, accessToken }): Promise<string> {
      const header = base64url(encoder.encode(JSON.stringify({ typ: "dpop+jwt", alg: "ES256", jwk: publicJwk })));
      const payload: Record<string, string | number> = {
        htu,
        htm: htm.toUpperCase(),
        iat: Math.floor(Date.now() / 1000),
        jti: randomUUID(),
      };
      if (nonce) payload.nonce = nonce;
      if (accessToken) payload.ath = base64url(await digest(accessToken));
      const signed = `${header}.${base64url(encoder.encode(JSON.stringify(payload)))}`;
      const signature = await webcrypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, keys.privateKey, encoder.encode(signed));
      return `${signed}.${base64url(signature)}`;
    },
    nativeFixtureKey() {
      return {
        private_key_b64: privateJwk.d!,
        public_jwk: { kty: publicJwk.kty!, crv: publicJwk.crv!, x: publicJwk.x!, y: publicJwk.y! },
      };
    },
  };
}

const pdsHost = (actor: Actor) => `${actor.pds}.test`;

async function listenForLoopbackCallback(): Promise<{
  readonly callback: Promise<{ code: string; state: string }>;
  close(): Promise<void>;
}> {
  let resolveCallback: ((value: { code: string; state: string }) => void) | undefined;
  const callback = new Promise<{ code: string; state: string }>((resolve) => { resolveCallback = resolve; });
  const server = createServer((request, response) => {
    const url = new URL(request.url ?? "/", "http://127.0.0.1:47191");
    if (request.method !== "GET" || url.pathname !== "/callback") {
      response.writeHead(404).end();
      return;
    }
    resolveCallback?.({ code: url.searchParams.get("code") ?? "", state: url.searchParams.get("state") ?? "" });
    console.info("[oauth-dpop] loopback callback received");
    response.writeHead(200, { "content-type": "text/plain" }).end("complete");
  });
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(47191, "127.0.0.1", () => {
      server.off("error", reject);
      resolve();
    });
  });
  return {
    callback,
    close: () => new Promise((resolve) => server.close(() => resolve())),
  };
}

async function request(host: string, url: URL, method: string, headers: Record<string, string>, body?: string): Promise<{ status: number; headers: https.IncomingHttpHeaders; body: string }> {
  return new Promise((resolve, reject) => {
    let req: https.ClientRequest;
    const timer = setTimeout(() => { req.destroy(new Error("PDS OAuth fixture request timed out")); }, 10_000);
    req = https.request({ host: "127.0.0.1", port: 443, servername: host, method, path: `${url.pathname}${url.search}`, rejectUnauthorized: false, headers: { host, ...headers } }, (res) => {
      let text = "";
      res.on("data", (chunk) => { text += String(chunk); });
      res.on("end", () => { clearTimeout(timer); resolve({ status: res.statusCode ?? 0, headers: res.headers, body: text }); });
    });
    req.on("error", (error) => { clearTimeout(timer); reject(error); });
    if (body) req.write(body);
    req.end();
  });
}

async function dpopRequest(host: string, url: URL, method: string, key: TestDpopKey, headers: Record<string, string>, body?: string, accessToken?: string) {
  let nonce: string | undefined;
  for (let attempt = 0; attempt < 2; attempt += 1) {
    const response = await request(host, url, method, { ...headers, dpop: await key.proof({ htu: url.toString(), htm: method, nonce, accessToken }) }, body);
    const error = (() => { try { return (JSON.parse(response.body) as { error?: string }).error; } catch { return undefined; } })();
    const challenge = typeof response.headers["dpop-nonce"] === "string" ? response.headers["dpop-nonce"] : undefined;
    if (error !== "use_dpop_nonce" || !challenge || attempt === 1) return response;
    nonce = challenge;
  }
  throw new Error("unreachable DPoP retry");
}

async function revokeThroughProductionCore(input: { readonly token: string; readonly client_id: string; readonly dpop_key: ReturnType<TestDpopKey["nativeFixtureKey"]> }): Promise<void> {
  const repoRoot = resolvePath(dirname(fileURLToPath(import.meta.url)), "../..");
  await new Promise<void>((resolve, reject) => {
    const child = spawn(
      "cargo",
      ["test", "-p", "opake-core", "client::oauth_token::tests::live_pds_revocation_from_stdin", "--features", "reqwest-transport", "--lib", "--", "--ignored", "--exact"],
      { cwd: repoRoot, stdio: ["pipe", "pipe", "ignore"] },
    );
    let output = "";
    child.stdout.on("data", (chunk) => { output += String(chunk); });
    const timer = setTimeout(() => {
      child.kill();
      reject(new Error("production revocation bridge timed out"));
    }, 60_000);
    child.once("error", (error) => { clearTimeout(timer); reject(error); });
    child.once("exit", (code) => {
      clearTimeout(timer);
      if (code === 0 && /test result: ok\. 1 passed;/u.test(output)) resolve();
      else if (code === 0) reject(new Error("production revocation bridge did not run exactly one test"));
      else reject(new Error(`production revocation bridge exited ${code ?? "without a status"}`));
    });
    child.stdin.end(JSON.stringify(input));
  });
}

/** Obtain an isolated identity-scoped real-PDS grant with a test-held DPoP key. */
export async function beginRealPdsIdentityGrant(page: Page, actor: Actor) {
  page.setDefaultTimeout(10_000);
  let browserStage = "opening authorization";
  page.on("framenavigated", (frame) => {
    if (frame === page.mainFrame()) {
      const location = new URL(frame.url());
      console.info(`[oauth-dpop] navigation ${location.origin}${location.pathname}`);
    }
  });
  page.on("requestfailed", (request) => {
    if (!request.isNavigationRequest()) return;
    const location = new URL(request.url());
    console.info(`[oauth-dpop] navigation failed ${location.origin}${location.pathname}: ${request.failure()?.errorText ?? "unknown"}`);
  });
  const host = pdsHost(actor);
  const metadataUrl = new URL("https://" + host + "/.well-known/oauth-authorization-server");
  const metadataResponse = await request(host, metadataUrl, "GET", {});
  if (metadataResponse.status !== 200) throw new Error("PDS OAuth metadata unavailable");
  const metadata = JSON.parse(metadataResponse.body) as { pushed_authorization_request_endpoint: string; authorization_endpoint: string; token_endpoint: string };
  const key = await generateTestDpopKey();
  const pkce = await generatePkcePair();
  const redirectUri = "http://127.0.0.1:47191/callback";
  const clientId = `http://localhost?redirect_uri=${encodeURIComponent(redirectUri)}&scope=${encodeURIComponent(CLIENT_METADATA_SCOPE)}`;
  const par = new URL(metadata.pushed_authorization_request_endpoint);
  const form = new URLSearchParams({ client_id: clientId, redirect_uri: redirectUri, response_type: "code", scope: "atproto identity:*", code_challenge: pkce.challenge, code_challenge_method: "S256", state: randomUUID() });
  const parResponse = await dpopRequest(host, par, "POST", key, { "content-type": "application/x-www-form-urlencoded" }, form.toString());
  if (parResponse.status !== 200 && parResponse.status !== 201) throw new Error(`PDS PAR refused with HTTP ${parResponse.status}`);
  const requestUri = (JSON.parse(parResponse.body) as { request_uri: string }).request_uri;
  const loopback = await listenForLoopbackCallback();
  const callbackWithin = async (): Promise<{ code: string; state: string }> => new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`PDS OAuth callback did not arrive during ${browserStage}`)), 20_000);
    void loopback.callback.then((value) => { clearTimeout(timer); resolve(value); });
  });
  let code: { code: string; state: string };
  try {
    await page.goto(`${metadata.authorization_endpoint}?client_id=${encodeURIComponent(clientId)}&request_uri=${encodeURIComponent(requestUri)}`, { waitUntil: "domcontentloaded" });
    const signIn = page.getByRole("button", { name: /^sign in$/i });
    const signInVisible = await signIn.waitFor({ state: "visible", timeout: 8_000 }).then(() => true).catch(() => false);
    if (signInVisible) {
      browserStage = "signing in";
      console.info("[oauth-dpop] sign-in form opened");
      await signIn.click();
      const identifier = page.locator('input[type="text"], input[type="email"]').first();
      const password = page.locator('input[type="password"]').first();
      await identifier.waitFor({ state: "visible", timeout: 8_000 });
      await password.waitFor({ state: "visible", timeout: 8_000 });
      await identifier.fill(actor.handle);
      await password.fill(actor.password);
      await page.locator('button[type="submit"]').first().click();
      console.info("[oauth-dpop] sign-in submitted");
    }
    const authorize = page.getByRole("button", { name: /authorize|allow|accept/i }).first();
    if (await authorize.waitFor({ state: "visible", timeout: 8_000 }).then(() => true).catch(() => false)) {
      browserStage = "submitting consent";
      await authorize.click();
      console.info("[oauth-dpop] consent submitted");
    } else {
      browserStage = "waiting for automatic consent";
      console.info("[oauth-dpop] no consent control; awaiting automatic redirect");
    }
    code = await callbackWithin();
  } finally {
    await loopback.close();
  }
  if (!code.code || code.state !== form.get("state")) throw new Error("PDS OAuth callback binding failed");
  const tokenEndpoint = new URL(metadata.token_endpoint);
  const exchange = new URLSearchParams({ grant_type: "authorization_code", code: code.code, redirect_uri: redirectUri, client_id: clientId, code_verifier: pkce.verifier });
  const tokenResponse = await dpopRequest(host, tokenEndpoint, "POST", key, { "content-type": "application/x-www-form-urlencoded" }, exchange.toString());
  if (tokenResponse.status !== 200) throw new Error("PDS token exchange refused");
  const accessToken = (JSON.parse(tokenResponse.body) as { access_token: string }).access_token;
  const protectedUrl = new URL(`https://${host}/xrpc/com.atproto.identity.getRecommendedDidCredentials`);
  const protectedRead = async (): Promise<number> => {
    const response = await dpopRequest(host, protectedUrl, "GET", key, { authorization: `DPoP ${accessToken}` }, undefined, accessToken);
    return response.status;
  };
  const revokeWithProductionCore = (): Promise<void> => revokeThroughProductionCore({
    token: accessToken,
    client_id: clientId,
    dpop_key: key.nativeFixtureKey(),
  });
  return { protectedRead, revokeWithProductionCore };
}
