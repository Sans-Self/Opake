// Authentication types for the two-step OAuth login flow.
//
// All auth logic lives in Opake's static methods (startLogin, completeLogin,
// login, loginWithAppPassword), which delegate to WASM exports. This file
// only defines the types that consumers need for the redirect round-trip.
//
// !! SECURITY BOUNDARY !!
//
// Tokens, DPoP keys, and session credentials are handled exclusively in WASM.
// JS strings are immutable and GC'd on the runtime's schedule — they cannot
// be zeroized. The WASM layer (opake-core) auto-zeroizes all sensitive types
// on drop via RedactedDebug + Zeroize.
//
// The one exception: PendingLogin state crosses the WASM/JS boundary because
// it must survive a full-page redirect via sessionStorage. This includes the
// DPoP key and PKCE verifier. Once completeLogin is called, those values
// enter WASM and the resulting session (tokens, keys) never leaves.

import type { DpopKeyPair, Storage } from "./storage";

/** Serializable state for the two-step login flow (survives page redirects). */
export interface PendingLogin {
  readonly pdsUrl: string;
  readonly did: string;
  readonly handle: string;
  readonly dpopKey: DpopKeyPair;
  readonly pkceVerifier: string;
  readonly csrfState: string;
  readonly tokenEndpoint: string;
  readonly clientId: string;
  readonly dpopNonce: string | null;
}

/** Options for the OAuth login flow. */
export interface LoginOptions {
  readonly storage: Storage;
  readonly redirectUri: string;
  /**
   * Platform-specific authorization step. Receives the auth URL,
   * must return the authorization code and state from the callback.
   *
   * Examples:
   * - Browser popup: open popup, listen for postMessage
   * - CLI: print URL, start localhost server, wait for callback
   * - Electron: open BrowserWindow, intercept redirect
   */
  readonly authorize: (authUrl: string) => Promise<{ code: string; state: string }>;
  /** Abort signal for timeout/cancellation. */
  readonly signal?: AbortSignal;
}

/** Options for the two-step login flow. */
export interface StartLoginOptions {
  readonly storage: Storage;
  readonly redirectUri: string;
}
