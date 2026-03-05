// Auth store — real OAuth 2.0 + DPoP flow via Zustand.
//
// State machine: initializing → unauthenticated ↔ authenticating → ready
//                                                 ↘ error

import { create } from "zustand";
import { wrap, type Remote } from "comlink";
import type { CryptoApi } from "@/workers/crypto.worker";
import type { OAuthSession, Config } from "@/lib/storage-types";
import { IndexedDbStorage } from "@/lib/indexeddb-storage";
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

let workerInstance: Remote<CryptoApi> | null = null;

function getWorker(): Remote<CryptoApi> {
  if (!workerInstance) {
    const raw = new Worker(
      new URL("../workers/crypto.worker.ts", import.meta.url),
      { type: "module" },
    );
    workerInstance = wrap<CryptoApi>(raw);
  }
  return workerInstance;
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
      set({ phase: "ready", did, handle: account.handle, pdsUrl: account.pdsUrl });
    } catch {
      set({ phase: "unauthenticated" });
    }
  },

  startLogin: async (handle: string) => {
    set({ phase: "authenticating" });
    const worker = getWorker();

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
      const message = error instanceof Error ? error.message : String(error);
      set({ phase: "error", message });
    }
  },

  completeLogin: async (code: string, callbackState: string) => {
    if (get().phase === "ready") return;
    set({ phase: "authenticating" });
    const worker = getWorker();

    try {
      const pending = loadPendingState();
      if (!pending) throw new Error("No pending OAuth state — start login again");

      // CSRF verification
      if (callbackState !== pending.csrfState) {
        clearPendingState();
        throw new Error("OAuth state mismatch — possible CSRF attack");
      }

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

      // Generate identity keypair
      const identity = await worker.generateIdentity(did);

      // Publish public key to PDS
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

      // Persist everything to IndexedDB
      const config: Config = await storage.loadConfig().catch(() => ({
        defaultDid: null,
        accounts: {},
        appviewUrl: null,
      }));
      config.defaultDid = did;
      config.accounts[did] = { pdsUrl: pending.pdsUrl, handle: pending.handle };

      await storage.saveConfig(config);
      await storage.saveSession(did, session);
      await storage.saveIdentity(did, identity);

      clearPendingState();

      set({
        phase: "ready",
        did,
        handle: pending.handle,
        pdsUrl: pending.pdsUrl,
      });
    } catch (error) {
      clearPendingState();
      const message = error instanceof Error ? error.message : String(error);
      set({ phase: "error", message });
    }
  },

  logout: async () => {
    const current = get();
    if (current.phase === "ready") {
      try {
        await storage.removeAccount(current.did);
      } catch {
        // best-effort cleanup
      }
    }
    set({ phase: "unauthenticated" });
  },
}));
