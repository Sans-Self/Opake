// Auth store — OAuth 2.0 + DPoP + identity resolution via Zustand.
//
// Two independent state dimensions:
//   Session:  none | authenticating | active
//   Identity: unchecked | checking | fresh | remote_only | conflict | ready
//
// Session determines "can we talk to the PDS?"
// Identity determines "can we encrypt/decrypt?"

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { OAuthSession, Config } from "@/lib/storage-types";
import { IndexedDbStorage } from "@/lib/indexeddb-storage";
import { getCryptoWorker } from "@/lib/worker";
import { authenticatedXrpc } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import {
  resolveHandleToPds,
  discoverAuthorizationServer,
  pushedAuthorizationRequest,
  buildAuthorizationUrl,
  buildClientId,
  buildRedirectUri,
  exchangeCode,
  publishPublicKey,
  savePendingState,
  loadPendingState,
  clearPendingState,
  generateCsrfState,
} from "@/lib/oauth";

// ---------------------------------------------------------------------------
// State types
// ---------------------------------------------------------------------------

export type SessionState =
  | { status: "initializing" }
  | { status: "none" }
  | { status: "authenticating" }
  | { status: "active"; did: string; handle: string; pdsUrl: string }
  | { status: "error"; message: string };

export type IdentityState =
  | { status: "unchecked" }
  | { status: "checking" }
  | { status: "fresh" }
  | { status: "remote_only" }
  | { status: "conflict" }
  | { status: "ready" };

interface AuthActions {
  boot(): Promise<void>;
  startLogin(handle: string): Promise<void>;
  completeLogin(code: string, state: string): Promise<void>;
  checkIdentity(): Promise<void>;
  generateAndPublishIdentity(): Promise<void>;
  logout(): Promise<void>;
}

interface AuthState extends AuthActions {
  session: SessionState;
  identity: IdentityState;
}

// Re-export a combined snapshot for router context
export interface AuthSnapshot {
  session: SessionState;
  identity: IdentityState;
}

// ---------------------------------------------------------------------------
// Singletons (created once, shared across store actions)
// ---------------------------------------------------------------------------

const storage = new IndexedDbStorage();

function loading(key: string) {
  useAppStore.getState().addLoading(key);
  return () => useAppStore.getState().removeLoading(key);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Check if publicKey/self already exists on the PDS. */
async function fetchUpstreamPublicKey(
  pdsUrl: string,
  did: string,
  session: OAuthSession,
): Promise<string | null> {
  try {
    const response = await authenticatedXrpc(
      {
        pdsUrl,
        lexicon: `com.atproto.repo.getRecord?repo=${encodeURIComponent(did)}&collection=app.opake.publicKey&rkey=self`,
        method: "GET",
      },
      session,
    );
    // atproto encodes byte fields as { $bytes: "<base64>" }
    const record = (response as { value?: { publicKey?: { $bytes: string } | string } }).value;
    const raw = record?.publicKey;
    if (raw == null) return null;
    return typeof raw === "string" ? raw : raw.$bytes;
  } catch (error) {
    console.warn("[auth] publicKey/self lookup failed (treating as absent):", error);
    return null;
  }
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useAuthStore = create<AuthState>()(
  immer((set, get) => ({
    session: { status: "initializing" },
    identity: { status: "unchecked" },

    boot: async () => {
      const done = loading("boot");

      try {
        const config = await storage.loadConfig();
        if (!config.defaultDid) {
          set((draft) => {
            draft.session = { status: "none" };
          });
          return;
        }

        const did = config.defaultDid;
        const account = config.accounts[did] as
          | import("@/lib/storage-types").AccountConfig
          | undefined;
        if (!account) {
          set((draft) => {
            draft.session = { status: "none" };
          });
          return;
        }

        // Verify session exists
        await storage.loadSession(did);

        set((draft) => {
          draft.session = { status: "active", did, handle: account.handle, pdsUrl: account.pdsUrl };
        });
      } catch {
        set((draft) => {
          draft.session = { status: "none" };
        });
      } finally {
        done();
      }
    },

    checkIdentity: async () => {
      const { session } = get();
      if (session.status !== "active") return;

      const done = loading("identity-check");
      set((draft) => {
        draft.identity = { status: "checking" };
      });

      try {
        const oauthSession = await storage.loadSession(session.did);

        const [localIdentity, upstreamKey] = await Promise.all([
          storage.loadIdentity(session.did).catch(() => null),
          fetchUpstreamPublicKey(session.pdsUrl, session.did, oauthSession as OAuthSession),
        ]);

        const hasLocal = localIdentity !== null;
        const hasUpstream = upstreamKey !== null;

        if (!hasLocal && !hasUpstream) {
          set((draft) => {
            draft.identity = { status: "fresh" };
          });
        } else if (!hasLocal && hasUpstream) {
          set((draft) => {
            draft.identity = { status: "remote_only" };
          });
        } else if (hasLocal && hasUpstream && localIdentity.public_key !== upstreamKey) {
          set((draft) => {
            draft.identity = { status: "conflict" };
          });
        } else {
          set((draft) => {
            draft.identity = { status: "ready" };
          });
        }
      } catch (error) {
        console.error("[auth] checkIdentity failed:", error);
        set((draft) => {
          draft.identity = { status: "unchecked" };
        });
      } finally {
        done();
      }
    },

    generateAndPublishIdentity: async () => {
      const { session } = get();
      if (session.status !== "active") return;

      const done = loading("generate-identity");
      set((draft) => {
        draft.identity = { status: "checking" };
      });
      const worker = getCryptoWorker();

      try {
        const oauthSession = (await storage.loadSession(session.did)) as OAuthSession;
        const identity = await worker.generateIdentity(session.did);

        await publishPublicKey(
          session.pdsUrl,
          session.did,
          identity.public_key,
          identity.verify_key,
          oauthSession.accessToken,
          oauthSession.dpopKey,
          oauthSession.dpopNonce,
          worker,
        );

        await storage.saveIdentity(session.did, identity);

        set((draft) => {
          draft.identity = { status: "ready" };
        });
      } catch (error) {
        console.error("[auth] generateAndPublishIdentity failed:", error);
        set((draft) => {
          draft.identity = { status: "fresh" };
        });
      } finally {
        done();
      }
    },

    startLogin: async (handle: string) => {
      const done = loading("login");

      set((draft) => {
        draft.session = { status: "authenticating" };
      });
      const worker = getCryptoWorker();

      try {
        const { pdsUrl } = await resolveHandleToPds(handle);
        const asm = await discoverAuthorizationServer(pdsUrl);
        const dpopKey = await worker.generateDpopKeyPair();
        const pkce = await worker.generatePkce();
        const csrfState = generateCsrfState();
        const redirectUri = buildRedirectUri();
        const clientId = buildClientId(redirectUri);
        const parEndpoint = asm.pushed_authorization_request_endpoint ?? asm.token_endpoint;

        const { requestUri, dpopNonce } = await pushedAuthorizationRequest(
          parEndpoint,
          clientId,
          redirectUri,
          pkce.challenge,
          csrfState,
          dpopKey,
          null,
          worker,
        );

        savePendingState({
          pdsUrl,
          handle,
          dpopKey,
          pkceVerifier: pkce.verifier,
          csrfState,
          tokenEndpoint: asm.token_endpoint,
          clientId,
          dpopNonce,
        });

        const authUrl = buildAuthorizationUrl(asm.authorization_endpoint, clientId, requestUri);
        window.location.href = authUrl;
      } catch (error) {
        console.error("[auth] startLogin failed:", error);
        const message = error instanceof Error ? error.message : String(error);
        set((draft) => {
          draft.session = { status: "error", message };
        });
      } finally {
        done();
      }
    },

    completeLogin: async (code: string, callbackState: string) => {
      const { session } = get();
      if (session.status === "active") return;
      const done = loading("complete-login");
      set((draft) => {
        draft.session = { status: "authenticating" };
      });
      const worker = getCryptoWorker();

      try {
        const pending = loadPendingState();
        if (!pending) throw new Error("No pending OAuth state — start login again");

        if (callbackState !== pending.csrfState) {
          clearPendingState();
          throw new Error("OAuth state mismatch — possible CSRF attack");
        }

        const redirectUri = buildRedirectUri();

        const { tokenResponse, dpopNonce } = await exchangeCode(
          pending.tokenEndpoint,
          pending.clientId,
          code,
          redirectUri,
          pending.pkceVerifier,
          pending.dpopKey,
          pending.dpopNonce,
          worker,
        );

        const did = tokenResponse.sub;
        if (!did) throw new Error("Token response missing `sub` claim");

        const timestamp = Math.floor(Date.now() / 1000);
        const expiresAt = tokenResponse.expires_in ? timestamp + tokenResponse.expires_in : null;

        const oauthSession: Readonly<OAuthSession> = {
          type: "oauth",
          did,
          handle: pending.handle,
          accessToken: tokenResponse.access_token,
          refreshToken: tokenResponse.refresh_token ?? "",
          dpopKey: pending.dpopKey,
          tokenEndpoint: pending.tokenEndpoint,
          dpopNonce,
          expiresAt,
          clientId: pending.clientId,
        };

        const existingConfig: Readonly<Config> = await storage.loadConfig().catch(() => ({
          defaultDid: null,
          accounts: {},
          appviewUrl: null,
        }));
        const config: Readonly<Config> = {
          ...existingConfig,
          defaultDid: did,
          accounts: {
            ...existingConfig.accounts,
            [did]: { pdsUrl: pending.pdsUrl, handle: pending.handle },
          },
        };

        await storage.saveConfig(config);
        await storage.saveSession(did, oauthSession);
        clearPendingState();

        set((draft) => {
          draft.session = { status: "active", did, handle: pending.handle, pdsUrl: pending.pdsUrl };
          draft.identity = { status: "unchecked" };
        });
      } catch (error) {
        console.error("[auth] completeLogin failed:", error);
        clearPendingState();
        const message = error instanceof Error ? error.message : String(error);
        set((draft) => {
          draft.session = { status: "error", message };
        });
      } finally {
        done();
      }
    },

    logout: async () => {
      const { session } = get();
      if (session.status === "active") {
        try {
          await storage.removeAccount(session.did);
        } catch {
          // best-effort cleanup
        }
      }
      set((draft) => {
        draft.session = { status: "none" };
        draft.identity = { status: "unchecked" };
      });
    },
  })),
);
