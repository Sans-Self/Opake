// Pre-seed CLI config files to bypass interactive login.
//
// Writes config.toml, session.json, and identity.json to a temp directory,
// then publishes the public key to the fake PDS. The CLI can then run
// commands with --config-dir pointing at this directory.

import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { getPds } from "./pds.js";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import initWasm, {
  deriveIdentityFromMnemonic,
} from "../../web/src/wasm/opake-wasm/opake.js";

// Known seed phrase for deterministic test identities
const TEST_SEED_PHRASE =
  "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";

// eslint-disable-next-line functional/no-let
let wasmInitialized = false;

async function ensureWasm(): Promise<void> {
  if (!wasmInitialized) {
    // Load WASM binary directly (the default init uses fetch which doesn't work in Node/Bun)
    const wasmPath = resolve(
      import.meta.dirname,
      "../../web/src/wasm/opake-wasm/opake_bg.wasm",
    );
    const wasmBytes = readFileSync(wasmPath);
    await initWasm(wasmBytes);
    wasmInitialized = true;
  }
}

interface AccountContext {
  readonly configDir: string;
  readonly did: string;
  readonly handle: string;
  readonly pdsUrl: string;
}

/** Sanitize a DID for use as a directory name: did:plc:abc → did_plc_abc */
function sanitizeDid(did: string): string {
  return did.replace(/:/g, "_");
}

/**
 * Set up a fully configured account directory for CLI testing.
 * Bypasses the interactive login flow by writing config files directly.
 */
export async function setupAccount(
  did: string,
  handle: string,
  opts?: { skipIdentity?: boolean },
): Promise<AccountContext> {
  await ensureWasm();
  const pds = getPds();
  const pdsUrl = pds.url;

  // Create temp config directory
  const configDir = mkdtempSync(join(tmpdir(), "opake-e2e-"));
  const accountDir = join(configDir, "accounts", sanitizeDid(did));
  mkdirSync(accountDir, { recursive: true });

  // 1. Generate identity from seed phrase via WASM
  const identity = deriveIdentityFromMnemonic(TEST_SEED_PHRASE, did) as {
    did: string;
    public_key: string;
    private_key: string;
    signing_key: string;
    verify_key: string;
  };

  // 2. Get a session from fake-pds
  const sessionRes = await fetch(`${pdsUrl}/xrpc/com.atproto.server.createSession`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ identifier: handle, password: "test" }),
  });
  const session = (await sessionRes.json()) as {
    accessJwt: string;
    refreshJwt: string;
    did: string;
    handle: string;
  };

  // 3. Write config.toml
  const configToml = `
default_did = "${did}"

[accounts.${JSON.stringify(did)}]
pds_url = "${pdsUrl}"
handle = "${handle}"
`.trim();
  writeFileSync(join(configDir, "config.toml"), configToml, { mode: 0o600 });

  // 4. Write session.json (legacy format — camelCase, matches Rust serde)
  const sessionJson = {
    type: "legacy",
    did: session.did,
    handle: session.handle,
    accessJwt: session.accessJwt,
    refreshJwt: session.refreshJwt,
  };
  writeFileSync(join(accountDir, "session.json"), JSON.stringify(sessionJson), {
    mode: 0o600,
  });

  // 5. Write identity.json (skip for pairing test — new device has no identity)
  if (!opts?.skipIdentity) {
    writeFileSync(join(accountDir, "identity.json"), JSON.stringify(identity), {
      mode: 0o600,
    });
  }

  // 6. Publish public key to fake PDS (skip if no identity)
  if (!opts?.skipIdentity) {
    const publicKeyRecord: Record<string, unknown> = {
      $type: "app.opake.publicKey",
      opakeVersion: 1,
      publicKey: { $bytes: identity.public_key },
      algo: "x25519",
      createdAt: new Date().toISOString(),
    };
    if (identity.verify_key) {
      publicKeyRecord["signingKey"] = { $bytes: identity.verify_key };
      publicKeyRecord["signingAlgo"] = "ed25519";
    }

    await fetch(`${pdsUrl}/xrpc/com.atproto.repo.putRecord`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${session.accessJwt}`,
      },
      body: JSON.stringify({
        repo: did,
        collection: "app.opake.publicKey",
        rkey: "self",
        record: publicKeyRecord,
      }),
    });
  }

  return { configDir, did, handle, pdsUrl };
}
