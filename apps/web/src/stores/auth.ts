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
import type { Opake, PendingLogin } from "@opake/sdk";
import type { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";

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
}

type AuthStore = AuthState & AuthActions;

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useAuthStore = create<AuthStore>()(
  immer((set) => ({
    session: { status: "initializing" } as SessionState,
    identity: { status: "none" } as IdentityState,

    async boot() {
      // Deduplicate concurrent calls (StrictMode, HMR, multiple route guards)
      if (bootPromise) {
        await bootPromise;
        return;
      }

      bootPromise = (async () => {
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
        } catch (err) {
          set((draft) => {
            draft.session = {
              status: "error",
              message: err instanceof Error ? err.message : "Failed to initialize",
            };
          });
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

        // Reset boot promise so it doesn't return stale state
        bootPromise = null;
      } catch (err) {
        set((draft) => {
          draft.session = {
            status: "error",
            message: err instanceof Error ? err.message : "Login failed. Please try again.",
          };
        });
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
  })),
);
