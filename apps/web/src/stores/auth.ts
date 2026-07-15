// Auth store — OAuth 2.0 + DPoP login via @opake/sdk.
//
// State machine:
//   Session:  initializing | none | authenticating | active | error
//   Identity: none | fresh | remote_only | conflict | ready
//
// The Opake instance lives at module level, accessed via getOpake().
// Storage is a lazy singleton shared across boot/login/callback.

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { Opake, ResolvedIdentity } from "@opake/sdk";
import { OpakeError } from "@opake/sdk";
import type { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";
import { base64ToUint8Array } from "@/lib/encoding";
import { ensurePersistentStorage } from "@/lib/persistent-storage";
import { loading } from "@/stores/app";

// Detect auth errors that indicate a dead session (stale/revoked tokens).
// Duck-typed to avoid importing OpakeError eagerly.
const DEAD_SESSION_SIGNALS = ["401", "AuthenticationFailed", "invalid_grant"];

// Bluesky appview for cosmetic profile (avatar/banner) lookups. Overridable so
// a hermetic dev-env can point it local or disable it (empty = skip) — without
// it the fetch escapes to live public.api.bsky.app, which the e2e blockade fails.
const PUBLIC_API =
  (import.meta.env.VITE_BSKY_APPVIEW_URL as string | undefined) ?? "https://public.api.bsky.app";

/**
 * Accept only HTTPS origins — plus loopback HTTP for the hermetic dev-env —
 * before a URL reaches `fetch` or `window.location`. Blocks `javascript:`,
 * `data:`, and `file:` targets a hostile PDS/auth-server could otherwise
 * smuggle into a redirect (script-URL XSS) or a probe request.
 */
function isSafeHttpUrl(url: URL): boolean {
  if (url.protocol === "https:") return true;
  return url.protocol === "http:" && (url.hostname === "localhost" || url.hostname === "127.0.0.1");
}

/** Validate an external navigation target before handing it to the browser. */
function assertSafeRedirect(target: string): string {
  if (!isSafeHttpUrl(new URL(target))) {
    throw new Error("Unsafe redirect target rejected");
  }
  return target;
}

/** Only accept avatar/banner URLs from known Bluesky CDN origins. */
function isSafeCdnUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "https:" && parsed.hostname.endsWith(".bsky.app");
  } catch {
    return false;
  }
}

/** Fire-and-forget profile fetch — populates avatarUrl/bannerUrl after session is active. */
function fetchProfileInBackground(did: string): void {
  if (!PUBLIC_API) return; // disabled (e.g. hermetic dev-env)
  void fetch(`${PUBLIC_API}/xrpc/app.bsky.actor.getProfile?actor=${encodeURIComponent(did)}`)
    .then((r) => (r.ok ? (r.json() as Promise<{ avatar?: string; banner?: string }>) : null))
    .then((profile) => {
      if (!profile) return;
      useAuthStore.setState((draft) => {
        if (draft.session.status !== "active" || draft.session.did !== did) return;
        draft.session.avatarUrl =
          profile.avatar && isSafeCdnUrl(profile.avatar) ? profile.avatar : null;
        draft.session.bannerUrl =
          profile.banner && isSafeCdnUrl(profile.banner) ? profile.banner : null;
      });
    })
    .catch((err: unknown) => {
      console.debug("[auth] profile fetch failed:", err);
    });
}

function isDeadSessionError(err: unknown): boolean {
  if (!err || typeof err !== "object") return false;
  const msg = "message" in err && typeof err.message === "string" ? err.message : "";
  return DEAD_SESSION_SIGNALS.some((s) => msg.includes(s));
}

// Lazy SDK imports — static imports trigger WASM evaluation during SSR.
const loadSdk = () => import("@opake/sdk");
const loadStorage = () => import("@opake/sdk/storage/indexeddb");

/**
 * Seed the indexer URL into a freshly-initialized Opake instance.
 *
 * The WASM binary ships with a compile-time `DEFAULT_INDEXER_URL`
 * baked in via `OPAKE_INDEXER_URL`, but one binary serves multiple
 * web deployments — staging, prod, local dev — so the runtime
 * `VITE_INDEXER_URL` has to win. Later writes to `accountConfig` on
 * the PDS override this value via `set_account_config` inside core,
 * so a user-configured indexer still beats the host default.
 */
async function seedIndexerUrl(opake: import("@opake/sdk").Opake): Promise<void> {
  await seedPlcDirectoryUrl();
  const envUrl = import.meta.env.VITE_INDEXER_URL as string | undefined;
  if (!envUrl) return;
  try {
    await opake.setIndexerUrl(envUrl);
  } catch (err) {
    console.warn("[auth] setIndexerUrl failed:", err);
  }
}

// Point the WASM did:plc resolver at a dev-env-local PLC directory when
// configured (VITE_PLC_DIRECTORY_URL). The override is process-level set-once
// (OnceLock in core), so this guards to a single effective call and must run
// before any handle/DID resolution — seedIndexerUrl (its only caller) runs at
// every Opake.init, ahead of login. Without it the browser WASM falls back to
// production plc.directory; in the e2e harness the route blockade fails loudly
// on that escape.
// eslint-disable-next-line functional/no-let
let plcDirectorySeeded = false;
async function seedPlcDirectoryUrl(): Promise<void> {
  if (plcDirectorySeeded) return;
  plcDirectorySeeded = true;
  const url = import.meta.env.VITE_PLC_DIRECTORY_URL as string | undefined;
  if (!url) return;
  try {
    const { Opake } = await loadSdk();
    await Opake.setPlcDirectoryUrl(url);
  } catch (err) {
    console.warn("[auth] setPlcDirectoryUrl failed:", err);
  }
}

// ---------------------------------------------------------------------------
// Module-level singletons
// ---------------------------------------------------------------------------

// eslint-disable-next-line functional/no-let
let storage: IndexedDbStorage | null = null;
// eslint-disable-next-line functional/no-let, functional/prefer-immutable-types
let opakeInstance: Opake | null = null;
// eslint-disable-next-line functional/no-let
let bootPromise: Promise<void> | null = null;

export async function getStorage(): Promise<IndexedDbStorage> {
  if (!storage) {
    // Request persistent storage BEFORE first IDB write. Without this,
    // the browser may evict our identity keys under disk pressure,
    // forcing full seed-phrase recovery. Fire-and-forget — the outcome
    // is logged and memoized; we don't block boot on the browser's
    // permission decision.
    void ensurePersistentStorage();

    const { IndexedDbStorage } = await loadStorage();
    storage ??= new IndexedDbStorage();
  }
  return storage;
}

/** Access the initialized Opake instance. Throws if not yet booted. */
export function getOpake(): Opake {
  if (!opakeInstance) throw new Error("Opake not initialized — call boot() first");
  return opakeInstance;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

function redirectUri(): string {
  return `${window.location.origin}/devices/oauth-callback`;
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type SessionState =
  | { status: "initializing" }
  | { status: "none" }
  | { status: "authenticating" }
  | {
      status: "active";
      did: string;
      handle: string;
      pdsUrl: string;
      avatarUrl: string | null;
      bannerUrl: string | null;
    }
  | { status: "error"; message: string };

export type IdentityState =
  | { status: "pending" }
  | { status: "none" }
  | { status: "fresh" }
  | { status: "remote_only" }
  | { status: "conflict" }
  | { status: "ready" };

interface AuthState {
  session: SessionState;
  identity: IdentityState;
}

interface AuthActions {
  boot(): Promise<void>;
  startLogin(handle: string): Promise<void>;
  completeLogin(code: string, state: string): Promise<void>;
  logout(): Promise<void>;

  // Identity lifecycle
  generateSeedPhrase(): Promise<string>;
  validateSeedPhrase(phrase: string): Promise<boolean>;
  saveIdentity(seedPhrase: string): Promise<void>;
  /**
   * Called after `Opake.awaitPairCompletion` resolves. The identity is
   * already persisted inside WASM's storage write; this refreshes the
   * auth store so the rest of the app picks it up.
   */
  finalizePairing(): Promise<void>;
  publishPublicKey(): Promise<void>;
}

export type AuthSnapshot = AuthState;

type AuthStore = AuthState & AuthActions;

// ---------------------------------------------------------------------------
// Identity resolution
// ---------------------------------------------------------------------------

function requireActiveDid(): string {
  const { session } = useAuthStore.getState();
  if (session.status !== "active") {
    throw new Error("No active session");
  }
  return session.did;
}

/** Fetch remote + resolve in one call. Used by post-boot flows that don't have a cached probe. */
async function fetchAndResolveIdentity(
  opake: Opake,
  did: string,
  storage: IndexedDbStorage,
): Promise<IdentityState> {
  const remote = await opake.resolveIdentity(did).catch(() => null);
  return resolveIdentityState(storage, did, remote);
}

async function resolveIdentityState(
  storage: IndexedDbStorage,
  did: string,
  remote: ResolvedIdentity | null,
): Promise<IdentityState> {
  const localIdentity = await storage.loadIdentity(did).catch(() => null);

  if (!localIdentity && !remote?.x25519PublicKey) return { status: "none" };
  if (!localIdentity) return { status: "remote_only" };
  if (!remote?.x25519PublicKey) return { status: "fresh" };

  // Both halves of the hybrid bundle are tied to the same mnemonic, so a
  // mismatch on either half indicates the local identity belongs to a
  // different recovery — surface as a conflict either way.
  const localX25519 = base64ToUint8Array(localIdentity.x25519_public_key);
  const localMlKem = base64ToUint8Array(localIdentity.ml_kem_public_key);
  const x25519Match =
    localX25519.length === remote.x25519PublicKey.length &&
    localX25519.every((b, i) => b === remote.x25519PublicKey[i]);
  const mlKemMatch =
    localMlKem.length === remote.mlKemPublicKey.length &&
    localMlKem.every((b, i) => b === remote.mlKemPublicKey[i]);

  return x25519Match && mlKemMatch ? { status: "ready" } : { status: "conflict" };
}

/**
 * Resolve identity state when no Opake instance is available — i.e. when
 * `Opake.init` threw `IdentityMissing` because there's no local identity on
 * this device yet. We can't call `opake.resolveIdentity(...)` without an
 * instance, so probe the PDS directly for `at.opake.publicKey/self` to
 * distinguish `remote_only` (user has an Opake identity elsewhere, needs
 * recovery) from `none` (genuinely fresh account, needs identity creation).
 *
 * `com.atproto.repo.getRecord` is unauthenticated, so no session is needed.
 */
async function resolveIdentityStateWithoutOpake(
  storage: IndexedDbStorage,
  did: string,
  pdsUrl: string,
): Promise<IdentityState> {
  const hasRemoteKey = await probeRemotePublicKey(pdsUrl, did);
  // `resolveIdentityState` handles the local-identity lookup + match logic;
  // pass a minimal remote shape reflecting only whether a key exists.
  // The empty-bytes sentinels are never byte-compared — `resolveIdentityState`
  // overrides any conflict to `remote_only` when no local identity exists.
  const remote: ResolvedIdentity | null = hasRemoteKey
    ? ({
        x25519PublicKey: new Uint8Array(0),
        mlKemPublicKey: new Uint8Array(0),
      } as ResolvedIdentity)
    : null;
  const state = await resolveIdentityState(storage, did, remote);
  // With no local identity, `resolveIdentityState` returns `remote_only` for
  // any non-null remote — the empty-publicKey sentinel is never compared.
  // If somehow a local identity also exists (unusual but possible mid-flow),
  // fall back to `remote_only` so the user lands on the recovery UI rather
  // than a bogus `conflict` computed against an empty key.
  return state.status === "conflict" ? { status: "remote_only" } : state;
}

async function probeRemotePublicKey(pdsUrl: string, did: string): Promise<boolean> {
  const url = new URL("/xrpc/com.atproto.repo.getRecord", pdsUrl);
  url.searchParams.set("repo", did);
  url.searchParams.set("collection", "at.opake.publicKey");
  url.searchParams.set("rkey", "self");
  if (!isSafeHttpUrl(url)) return false;
  try {
    const res = await fetch(url);
    return res.ok;
  } catch {
    // Network failure during the probe. Defaulting to `false` lands the user
    // on the fresh-account flow; a real remote key will surface on the next
    // boot when the probe retries.
    return false;
  }
}

/**
 * Outcome of attempting to open an Opake instance for a stored account.
 *
 * Boot and OAuth-callback both share the same three-way fork: either we
 * successfully constructed an Opake (happy path), we have a valid session
 * but no local identity yet (the user needs to recover or pair), or the
 * session itself is unusable (missing, revoked, corrupt). Modelling the
 * outcome as a discriminated union keeps the call sites linear.
 */
type OpakeInitResult =
  | { readonly kind: "opake"; readonly opake: Opake }
  | { readonly kind: "identity-missing" }
  | { readonly kind: "signed-out" };

/**
 * Try to build an Opake for `did`, classifying the outcome. Also runs the
 * liveness probe (`checkSession`) — a dead session degrades to `signed-out`
 * so the caller doesn't have to repeat the dead-token cleanup dance.
 *
 * The `seedIndexerUrl` call runs on the happy path so the returned Opake is
 * ready to use immediately; callers just need to assign it to the module
 * singleton and carry on.
 */
async function classifyOpakeInit(
  OpakeCtor: typeof Opake,
  storage: IndexedDbStorage,
  did: string,
): Promise<OpakeInitResult> {
  try {
    const opake = await OpakeCtor.init({ storage });
    await seedIndexerUrl(opake);
    return await classifySession(opake, storage, did);
  } catch (err) {
    if (err instanceof OpakeError && err.kind === "IdentityMissing") {
      return { kind: "identity-missing" };
    }
    return { kind: "signed-out" };
  }
}

/**
 * Liveness-check an already-constructed Opake. Opake.init loads tokens from
 * storage without validating them — stale or revoked tokens only fail on the
 * first real XRPC call. `checkSession` forces that round-trip early so a
 * dead session doesn't limp into the app and fail somewhere less obvious.
 */
async function classifySession(
  opake: Opake,
  storage: IndexedDbStorage,
  did: string,
): Promise<OpakeInitResult> {
  try {
    await opake.checkSession();
    return { kind: "opake", opake };
  } catch (err) {
    if (isDeadSessionError(err)) {
      opake.destroy();
      await storage.clearSession(did).catch(() => {
        /* best effort — storage may already be gone */
      });
      return { kind: "signed-out" };
    }
    // Non-auth error (network blip, PDS hiccup) — keep the instance, the
    // caller can retry when the user takes their next action.
    return { kind: "opake", opake };
  }
}

/** Derive identity from seed phrase, save, re-init Opake with the new identity. */
async function deriveAndPersistIdentity(seedPhrase: string, did: string): Promise<void> {
  const { Opake } = await loadSdk();
  const s = await getStorage();
  const identity = await Opake.createIdentity(seedPhrase, did);
  await s.saveIdentity(did, identity);

  const opake = await Opake.init({ storage: s, did });
  await seedIndexerUrl(opake);
  opakeInstance?.destroy();
  opakeInstance = opake;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useAuthStore = create<AuthStore>()(
  immer((set) => ({
    session: { status: "initializing" },
    identity: { status: "pending" },

    async boot() {
      // Deduplicate concurrent calls (StrictMode, HMR, multiple route guards)
      if (bootPromise) {
        await bootPromise;
        return;
      }

      bootPromise = (async () => {
        const done = loading("boot");
        try {
          const { Opake } = await loadSdk();
          const s = await getStorage();
          const configured = await Opake.isConfigured(s);

          if (!configured) {
            set((draft) => {
              draft.session = { status: "none" };
            });
            return;
          }

          const config = await s.loadConfig();
          const did = config.default_did;
          const account = did ? config.accounts[did] : undefined;

          if (!did || !account) {
            set((draft) => {
              draft.session = { status: "none" };
            });
            return;
          }

          // Opake.init can land in three distinct states:
          //   1. signed-out — session missing / dead / unreadable
          //   2. identity-missing — authenticated but no local keys yet
          //                         (user needs recovery or pairing)
          //   3. opake — happy path
          const result = await classifyOpakeInit(Opake, s, did);

          if (result.kind === "signed-out") {
            bootPromise = null;
            set((draft) => {
              draft.session = { status: "none" };
            });
            return;
          }

          if (result.kind === "opake") {
            opakeInstance = result.opake;
          }

          set((draft) => {
            draft.session = {
              status: "active",
              did,
              handle: account.handle,
              pdsUrl: account.pds_url,
              avatarUrl: null,
              bannerUrl: null,
            };
          });

          fetchProfileInBackground(did);

          const identityState =
            result.kind === "identity-missing"
              ? await resolveIdentityStateWithoutOpake(s, did, account.pds_url)
              : await fetchAndResolveIdentity(result.opake, did, s);
          set((draft) => {
            draft.identity = identityState;
          });
        } catch (err) {
          set((draft) => {
            draft.session = {
              status: "error",
              message: err instanceof Error ? err.message : "Failed to initialize",
            };
          });
        } finally {
          done();
        }
      })();

      await bootPromise;
    },

    async startLogin(handle) {
      // Double-click guard
      const current = useAuthStore.getState().session;
      if (current.status === "authenticating") return;

      set((draft) => {
        draft.session = { status: "authenticating" };
      });

      const done = loading("login");
      try {
        const { Opake } = await loadSdk();
        // Point the did:plc resolver at the dev-env PLC BEFORE the first
        // handle/DID resolution (startLogin resolves handle → did:plc → PDS).
        await seedPlcDirectoryUrl();
        const { authUrl, pending } = await Opake.startLogin(handle, {
          redirectUri: redirectUri(),
        });

        Opake.savePendingLogin(pending);
        window.location.href = assertSafeRedirect(authUrl);
      } catch (err) {
        set((draft) => {
          draft.session = {
            status: "error",
            message: err instanceof Error ? err.message : "Login failed",
          };
        });
      } finally {
        done();
      }
    },

    async completeLogin(code, state) {
      const { Opake } = await loadSdk();
      const pending = Opake.loadPendingLogin();

      if (!pending) {
        set((draft) => {
          draft.session = {
            status: "error",
            message: "Login session expired. Please try again.",
          };
        });
        return;
      }

      const done = loading("complete-login");
      try {
        const s = await getStorage();

        await Opake.completeLogin(code, state, pending, {
          storage: s,
          redirectUri: redirectUri(),
        });

        // Mark session active as soon as OAuth completes — the session is
        // persisted, so the login itself has succeeded. Identity bootstrap
        // (which may fail with `IdentityMissing` on a fresh device) is a
        // separate concern handled below.
        set((draft) => {
          draft.session = {
            status: "active",
            did: pending.did,
            handle: pending.handle,
            pdsUrl: pending.pdsUrl,
            avatarUrl: null,
            bannerUrl: null,
          };
        });

        fetchProfileInBackground(pending.did);

        // Try to construct an Opake instance. On a device that has logged in
        // to this account before, this succeeds and the full identity-state
        // resolution runs. On a fresh device, `Opake.init` throws
        // `IdentityMissing` — expected, not a login failure. Fall back to a
        // PDS-direct probe so the `/devices` route picks the right view
        // (`RecoverIdentityView` vs `FreshAccountView`).
        try {
          const opake = await Opake.init({ storage: s });
          await seedIndexerUrl(opake);
          opakeInstance = opake;

          const identityState = await fetchAndResolveIdentity(opake, pending.did, s);
          set((draft) => {
            draft.identity = identityState;
          });
        } catch (err) {
          if (err instanceof OpakeError && err.kind === "IdentityMissing") {
            const identityState = await resolveIdentityStateWithoutOpake(
              s,
              pending.did,
              pending.pdsUrl,
            );
            set((draft) => {
              draft.identity = identityState;
            });
          } else {
            throw err;
          }
        }

        // Reset boot promise so it doesn't return stale state
        bootPromise = null;
      } catch (err) {
        set((draft) => {
          draft.session = {
            status: "error",
            message: err instanceof Error ? err.message : "Login failed. Please try again.",
          };
        });
      } finally {
        done();
      }
    },

    async logout() {
      opakeInstance?.destroy();
      opakeInstance = null;
      bootPromise = null;

      try {
        const s = await getStorage();
        const config = await s.loadConfig();
        if (config.default_did) {
          await s.removeAccount(config.default_did);
        }
      } catch {
        // Best effort — storage might already be gone
      }

      set((draft) => {
        draft.session = { status: "none" };
        draft.identity = { status: "none" };
      });
    },

    // -----------------------------------------------------------------
    // Identity lifecycle
    // -----------------------------------------------------------------

    async generateSeedPhrase() {
      const { Opake } = await loadSdk();
      return Opake.generateSeedPhrase();
    },

    async validateSeedPhrase(phrase) {
      const { Opake } = await loadSdk();
      return Opake.validateSeedPhrase(phrase);
    },

    async saveIdentity(seedPhrase) {
      const did = requireActiveDid();
      const done = loading("save-identity");
      try {
        await deriveAndPersistIdentity(seedPhrase, did);

        const s = await getStorage();
        const identityState = await fetchAndResolveIdentity(getOpake(), did, s);
        set((draft) => {
          draft.identity = identityState;
        });
      } finally {
        done();
      }
    },

    async finalizePairing() {
      const did = requireActiveDid();
      const done = loading("finalize-pairing");
      try {
        const { Opake } = await loadSdk();
        const s = await getStorage();
        // `awaitPairCompletion` already wrote the identity to storage via
        // WASM — we just spin up an Opake handle and refresh store state.
        const opake = await Opake.init({ storage: s, did });
        await seedIndexerUrl(opake);
        opakeInstance?.destroy();
        opakeInstance = opake;

        const identityState = await fetchAndResolveIdentity(opake, did, s);
        set((draft) => {
          draft.identity = identityState;
        });
      } finally {
        done();
      }
    },

    async publishPublicKey() {
      requireActiveDid();
      const done = loading("publish-key");
      try {
        await getOpake().publishPublicKey();
        set((draft) => {
          draft.identity = { status: "ready" };
        });
      } finally {
        done();
      }
    },
  })),
);
