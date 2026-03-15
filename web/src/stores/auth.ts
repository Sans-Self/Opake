// Auth store — OAuth 2.0 + DPoP + identity resolution via Zustand.
//
// NOTE TO EDITORS:
// Opake uses a dual-documentation system. If you modify the authentication
// flow, identity state machine, or session persistence in this file, you
// MUST also update the corresponding MDX content in `web/src/content/`
// to prevent documentation drift.
//
// Two independent state dimensions:
//   Session:  none | authenticating | active
//   Identity: unchecked | checking | fresh | remote_only | conflict | ready
//
// Session determines "can we talk to the PDS?"
// Identity determines "can we encrypt/decrypt?"

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { OAuthSession, Config } from "@/lib/storageTypes";
import { IndexedDbStorage } from "@/lib/indexeddbStorage";
import { getCryptoWorker } from "@/lib/worker";
import { authenticatedXrpc, authenticatedPutRecord } from "@/lib/api";
import { loading } from "@/stores/app";
import {
  resolveHandleToPds,
  discoverAuthorizationServer,
  pushedAuthorizationRequest,
  buildAuthorizationUrl,
  buildClientId,
  buildRedirectUri,
  exchangeCode,
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
  generateSeedPhrase(): Promise<string>;
  confirmSeedPhrase(phrase: string): Promise<void>;
  recoverFromSeedPhrase(phrase: string, force?: boolean): Promise<{ mismatch: boolean }>;
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

interface ProfileUrls {
  readonly avatarUrl: string | null;
  readonly bannerUrl: string | null;
}

/** Fetch the user's Bluesky profile avatar + banner URLs via the PDS proxy. */
async function fetchProfileUrls(pdsUrl: string, did: string): Promise<ProfileUrls> {
  try {
    const res = await fetch(
      `${pdsUrl}/xrpc/app.bsky.actor.getProfile?actor=${encodeURIComponent(did)}`,
      { headers: { "atproto-proxy": "did:web:api.bsky.app#bsky_appview" } },
    );
    if (!res.ok) return { avatarUrl: null, bannerUrl: null };
    const profile = (await res.json()) as { avatar?: string; banner?: string };
    return {
      avatarUrl: profile.avatar ?? null,
      bannerUrl: profile.banner ?? null,
    };
  } catch {
    return { avatarUrl: null, bannerUrl: null };
  }
}

/**
 * Load cached profile, then refresh from the network in the background.
 * Calls `applyProfile` immediately with cached data (if any) and again after the fetch.
 */
function loadAndRefreshProfile(
  pdsUrl: string,
  did: string,
  applyProfile: (urls: ProfileUrls) => void,
): void {
  // Show cached data instantly
  void storage.loadProfile(did).then((cached) => {
    if (cached) applyProfile({ avatarUrl: cached.avatarUrl, bannerUrl: cached.bannerUrl });
  });

  // Then refresh from the network
  void fetchProfileUrls(pdsUrl, did).then((urls) => {
    applyProfile(urls);
    void storage.saveProfile(did, {
      avatarUrl: urls.avatarUrl,
      bannerUrl: urls.bannerUrl,
      fetchedAt: Date.now(),
    });
  });
}

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

/** Publish an identity's public key to the PDS via authenticated putRecord.
 *  Uses the token-refresh-aware `authenticatedPutRecord` path. */
async function publishIdentityKey(
  pdsUrl: string,
  did: string,
  identity: { readonly public_key: string; readonly verify_key: string | null },
  session: OAuthSession,
): Promise<void> {
  const record: Record<string, unknown> = {
    opakeVersion: 1,
    algo: "x25519",
    publicKey: { $bytes: identity.public_key },
    createdAt: new Date().toISOString(),
    ...(identity.verify_key
      ? { signingKey: { $bytes: identity.verify_key }, signingAlgo: "ed25519" }
      : {}),
  };

  await authenticatedPutRecord(
    { pdsUrl, did, collection: "app.opake.publicKey", rkey: "self", record },
    session,
  );
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
          | import("@/lib/storageTypes").AccountEntry
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
          draft.session = {
            status: "active",
            did,
            handle: account.handle,
            pdsUrl: account.pdsUrl,
            avatarUrl: null,
            bannerUrl: null,
          };
        });

        // Fire-and-forget — don't block boot on profile images
        loadAndRefreshProfile(account.pdsUrl, did, ({ avatarUrl, bannerUrl }) => {
          set((draft) => {
            if (draft.session.status === "active") {
              draft.session.avatarUrl = avatarUrl;
              draft.session.bannerUrl = bannerUrl;
            }
          });
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

        await publishIdentityKey(session.pdsUrl, session.did, identity, oauthSession);
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

    generateSeedPhrase: async () => {
      const worker = getCryptoWorker();
      return worker.generateMnemonic();
    },

    confirmSeedPhrase: async (phrase: string) => {
      const { session } = get();
      if (session.status !== "active") return;

      const done = loading("confirm-seed-phrase");
      const worker = getCryptoWorker();

      try {
        const oauthSession = (await storage.loadSession(session.did)) as OAuthSession;
        const identity = await worker.deriveIdentityFromMnemonic(phrase, session.did);

        await publishIdentityKey(session.pdsUrl, session.did, identity, oauthSession);
        await storage.saveIdentity(session.did, identity);

        set((draft) => {
          draft.identity = { status: "ready" };
        });
      } catch (error) {
        console.error("[auth] confirmSeedPhrase failed:", error);
        set((draft) => {
          draft.identity = { status: "fresh" };
        });
        throw error;
      } finally {
        done();
      }
    },

    recoverFromSeedPhrase: async (phrase: string, force?: boolean) => {
      const { session } = get();
      if (session.status !== "active") return { mismatch: false };

      const done = loading("recover-seed-phrase");
      const worker = getCryptoWorker();

      try {
        const oauthSession = (await storage.loadSession(session.did)) as OAuthSession;
        const identity = await worker.deriveIdentityFromMnemonic(phrase, session.did);

        // Compare against published key.
        const upstreamKey = await fetchUpstreamPublicKey(session.pdsUrl, session.did, oauthSession);

        if (upstreamKey && identity.public_key !== upstreamKey && !force) {
          return { mismatch: true };
        }

        set((draft) => {
          draft.identity = { status: "checking" };
        });

        await publishIdentityKey(session.pdsUrl, session.did, identity, oauthSession);
        await storage.saveIdentity(session.did, identity);

        set((draft) => {
          draft.identity = { status: "ready" };
        });

        return { mismatch: false };
      } catch (error) {
        console.error("[auth] recoverFromSeedPhrase failed:", error);
        set((draft) => {
          draft.identity = { status: "unchecked" };
        });
        throw error;
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
          handle,
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
          draft.session = {
            status: "active",
            did,
            handle: pending.handle,
            pdsUrl: pending.pdsUrl,
            avatarUrl: null,
            bannerUrl: null,
          };
          draft.identity = { status: "unchecked" };
        });

        loadAndRefreshProfile(pending.pdsUrl, did, ({ avatarUrl, bannerUrl }) => {
          set((draft) => {
            if (draft.session.status === "active") {
              draft.session.avatarUrl = avatarUrl;
              draft.session.bannerUrl = bannerUrl;
            }
          });
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
