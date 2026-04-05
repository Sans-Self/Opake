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
import type { Opake, PendingLogin, ResolvedIdentity } from "@opake/sdk";
import type { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";
import { base64ToUint8Array } from "@/lib/encoding";
import { loading } from "@/stores/app";

// Detect auth errors that indicate a dead session (stale/revoked tokens).
// Duck-typed to avoid importing OpakeError eagerly.
const DEAD_SESSION_SIGNALS = ["401", "AuthenticationFailed", "invalid_grant"];

function isDeadSessionError(err: unknown): boolean {
  if (!err || typeof err !== "object") return false;
  const msg = "message" in err && typeof err.message === "string" ? err.message : "";
  return DEAD_SESSION_SIGNALS.some((s) => msg.includes(s));
}

// Lazy SDK imports — static imports trigger WASM evaluation during SSR.
const loadSdk = () => import("@opake/sdk");
const loadStorage = () => import("@opake/sdk/storage/indexeddb");

// ---------------------------------------------------------------------------
// Module-level singletons
// ---------------------------------------------------------------------------

// eslint-disable-next-line functional/no-let
let storage: IndexedDbStorage | null = null;
// eslint-disable-next-line functional/no-let, functional/prefer-immutable-types
let opakeInstance: Opake | null = null;
// eslint-disable-next-line functional/no-let
let bootPromise: Promise<void> | null = null;

async function getStorage(): Promise<IndexedDbStorage> {
  if (!storage) {
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

const PENDING_KEY = "opake:pendingLogin";

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
  saveReceivedIdentity(identity: import("@opake/sdk").Identity): Promise<void>;
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

  if (!localIdentity && !remote?.publicKey) return { status: "none" };
  if (!localIdentity) return { status: "remote_only" };
  if (!remote?.publicKey) return { status: "fresh" };

  const localKeyBytes = base64ToUint8Array(localIdentity.public_key);
  const keysMatch =
    localKeyBytes.length === remote.publicKey.length &&
    localKeyBytes.every((b, i) => b === remote.publicKey[i]);

  return keysMatch ? { status: "ready" } : { status: "conflict" };
}

/** Derive identity from seed phrase, save, re-init Opake with the new identity. */
async function deriveAndPersistIdentity(seedPhrase: string, did: string): Promise<void> {
  const { Opake } = await loadSdk();
  const s = await getStorage();
  const identity = await Opake.createIdentity(seedPhrase, did);
  await s.saveIdentity(did, identity);

  const opake = await Opake.init({ storage: s, did });
  opakeInstance?.destroy();
  opakeInstance = opake;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useAuthStore = create<AuthStore>()(
  immer((set) => ({
    session: { status: "initializing" } as SessionState,
    identity: { status: "pending" } as IdentityState,

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

          // Opake.init loads the session — may not exist yet on the
          // OAuth callback page (config saved pre-redirect, session
          // saved by completeLogin after redirect).
          const opake = await Opake.init({ storage: s }).catch(() => null);
          if (!opake) {
            set((draft) => {
              draft.session = { status: "none" };
            });
            return;
          }
          opakeInstance = opake;

          // Probe: verify the session is actually usable. Opake.init()
          // loads tokens from storage without validating them — stale
          // or revoked tokens only fail on the first real XRPC call.
          try {
            await opake.checkSession();
          } catch (err) {
            if (isDeadSessionError(err)) {
              opakeInstance = null;
              opake.destroy();
              await s.clearSession(did).catch(() => {});
              bootPromise = null;
              set((draft) => {
                draft.session = { status: "none" };
              });
              return;
            }
            // Non-auth error (network blip, etc.) — continue
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

          const identityState = await fetchAndResolveIdentity(opake, did, s);
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
        const s = await getStorage();
        const { authUrl, pending } = await Opake.startLogin(handle, {
          storage: s,
          redirectUri: redirectUri(),
        });

        sessionStorage.setItem(PENDING_KEY, JSON.stringify(pending));
        window.location.href = authUrl;
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
      const raw = sessionStorage.getItem(PENDING_KEY);
      sessionStorage.removeItem(PENDING_KEY);

      if (!raw) {
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
        const { Opake } = await loadSdk();
        const pending = JSON.parse(raw) as PendingLogin;
        const s = await getStorage();

        await Opake.completeLogin(code, state, pending, {
          storage: s,
          redirectUri: redirectUri(),
        });

        const opake = await Opake.init({ storage: s });
        opakeInstance = opake;

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

        const identityState = await fetchAndResolveIdentity(opake, pending.did, s);
        set((draft) => {
          draft.identity = identityState;
        });

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

    async saveReceivedIdentity(identity) {
      const did = requireActiveDid();
      const done = loading("save-identity");
      try {
        const { Opake } = await loadSdk();
        const s = await getStorage();
        await s.saveIdentity(did, identity);

        const opake = await Opake.init({ storage: s, did });
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
