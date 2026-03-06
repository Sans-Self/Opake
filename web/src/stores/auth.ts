// Auth store — real OAuth 2.0 + DPoP flow via Zustand.
//
// State machine: initializing → unauthenticated ↔ authenticating → ready
//                                                 ↘ awaiting_identity
//                                                 ↘ error

import { create } from "zustand";
import type { OAuthSession, Config } from "@/lib/storage-types";
import { IndexedDbStorage } from "@/lib/indexeddb-storage";
import type { Remote } from "comlink";
import type { CryptoApi } from "@/workers/crypto.worker";
import { getCryptoWorker } from "@/lib/worker";
import { authenticatedXrpc } from "@/lib/api";
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

type AuthPhase =
  | { phase: "initializing" }
  | { phase: "unauthenticated" }
  | { phase: "authenticating" }
  | { phase: "ready"; did: string; handle: string; pdsUrl: string }
  | { phase: "awaiting_identity"; did: string; handle: string; pdsUrl: string }
  | { phase: "error"; message: string };

interface AuthActions {
  boot(): Promise<void>;
  startLogin(handle: string): Promise<void>;
  completeLogin(code: string, state: string): Promise<void>;
  logout(): Promise<void>;
}

type AuthState = AuthPhase & AuthActions;

// ---------------------------------------------------------------------------
// Singletons (created once, shared across store actions)
// ---------------------------------------------------------------------------

const storage = new IndexedDbStorage();

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Check if publicKey/self already exists on the PDS (i.e. another device published it). */
async function checkExistingPublicKey(
  pdsUrl: string,
  did: string,
  session: OAuthSession,
  _worker: Remote<CryptoApi>,
): Promise<boolean> {
  try {
    await authenticatedXrpc(
      {
        pdsUrl,
        lexicon: `com.atproto.repo.getRecord?repo=${encodeURIComponent(did)}&collection=app.opake.publicKey&rkey=self`,
        method: "GET",
      },
      session,
    );
    return true;
  } catch (error) {
    console.warn("[auth] publicKey/self lookup failed (treating as new account):", error);
    return false;
  }
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useAuthStore = create<AuthState>((set, get) => ({
  phase: "initializing",

  boot: async () => {
    try {
      const config = await storage.loadConfig();
      if (!config.defaultDid) {
        set({ phase: "unauthenticated" });
        return;
      }

      const did = config.defaultDid;
      const account = config.accounts[did];
      if (!account) {
        set({ phase: "unauthenticated" });
        return;
      }

      // Verify session exists
      await storage.loadSession(did);

      // Check if identity exists locally
      const hasIdentity = await storage
        .loadIdentity(did)
        .then(() => true)
        .catch(() => false);

      if (hasIdentity) {
        set({ phase: "ready", did, handle: account.handle, pdsUrl: account.pdsUrl });
      } else {
        set({ phase: "awaiting_identity", did, handle: account.handle, pdsUrl: account.pdsUrl });
      }
    } catch {
      set({ phase: "unauthenticated" });
    }
  },

  startLogin: async (handle: string) => {
    set({ phase: "authenticating" });
    const worker = getCryptoWorker();

    try {
      // Resolve handle → PDS
      const { did: _did, pdsUrl } = await resolveHandleToPds(handle);

      // OAuth discovery
      const asm = await discoverAuthorizationServer(pdsUrl);

      // Generate crypto material
      const dpopKey = await worker.generateDpopKeyPair();
      const pkce = await worker.generatePkce();
      const csrfState = generateCsrfState();

      // Client ID + redirect URI
      const redirectUri = buildRedirectUri();
      const clientId = buildClientId(redirectUri);

      const parEndpoint =
        asm.pushed_authorization_request_endpoint ?? asm.token_endpoint;

      // PAR
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

      // Persist pre-redirect state
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

      // Redirect to AS
      const authUrl = buildAuthorizationUrl(
        asm.authorization_endpoint,
        clientId,
        requestUri,
      );
      window.location.href = authUrl;
    } catch (error) {
      console.error("[auth] startLogin failed:", error);
      const message = error instanceof Error ? error.message : String(error);
      set({ phase: "error", message });
    }
  },

  completeLogin: async (code: string, callbackState: string) => {
    console.debug("[auth] completeLogin called, current phase:", get().phase);
    if (get().phase === "ready") return;
    set({ phase: "authenticating" });
    const worker = getCryptoWorker();

    try {
      const pending = loadPendingState();
      console.debug("[auth] pending state:", pending ? "loaded" : "missing");
      if (!pending) throw new Error("No pending OAuth state — start login again");

      // CSRF verification
      if (callbackState !== pending.csrfState) {
        clearPendingState();
        throw new Error("OAuth state mismatch — possible CSRF attack");
      }

      console.debug("[auth] CSRF ok, exchanging code");
      const redirectUri = buildRedirectUri();

      // Exchange code for tokens
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

      console.debug("[auth] token exchange done, sub:", tokenResponse.sub);
      const did = tokenResponse.sub;
      if (!did) throw new Error("Token response missing `sub` claim");

      const timestamp = Math.floor(Date.now() / 1000);
      const expiresAt = tokenResponse.expires_in
        ? timestamp + tokenResponse.expires_in
        : null;

      const session: OAuthSession = {
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

      console.debug("[auth] checking existing publicKey/self");
      // Check if this DID already has a publicKey/self record on the PDS.
      // If so, another device owns the identity — don't overwrite it.
      const hasExistingKey = await checkExistingPublicKey(
        pending.pdsUrl,
        did,
        session,
        worker,
      );

      // Persist config + session regardless
      const config: Config = await storage.loadConfig().catch(() => ({
        defaultDid: null,
        accounts: {},
        appviewUrl: null,
      }));
      config.defaultDid = did;
      config.accounts[did] = { pdsUrl: pending.pdsUrl, handle: pending.handle };

      await storage.saveConfig(config);
      await storage.saveSession(did, session);

      clearPendingState();

      console.debug("[auth] hasExistingKey:", hasExistingKey);

      if (hasExistingKey) {
        // Identity exists on PDS but not locally — user needs to pair
        set({
          phase: "awaiting_identity",
          did,
          handle: pending.handle,
          pdsUrl: pending.pdsUrl,
        });
      } else {
        // Fresh account — generate identity and publish key
        const identity = await worker.generateIdentity(did);

        await publishPublicKey(
          pending.pdsUrl,
          did,
          identity.publicKey,
          identity.verifyKey,
          session.accessToken,
          session.dpopKey,
          session.dpopNonce,
          worker,
        );

        await storage.saveIdentity(did, identity);

        set({
          phase: "ready",
          did,
          handle: pending.handle,
          pdsUrl: pending.pdsUrl,
        });
      }
    } catch (error) {
      console.error("[auth] completeLogin failed:", error);
      clearPendingState();
      const message = error instanceof Error ? error.message : String(error);
      set({ phase: "error", message });
    }
  },

  logout: async () => {
    const current = get();
    if (current.phase === "ready" || current.phase === "awaiting_identity") {
      try {
        await storage.removeAccount(current.did);
      } catch {
        // best-effort cleanup
      }
    }
    set({ phase: "unauthenticated" });
  },
}));
