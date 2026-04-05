// Opake — the main entry point for the SDK.
//
// Holds a long-lived WasmOpakeHandle (authenticated XRPC client + identity +
// storage). Create via `Opake.init()`, then call `.cabinet()` or
// `.workspaceFromKey()` to get a FileManager for file operations.
//
// All public async methods are decorated with @withTokenGuard, which
// proactively refreshes the OAuth token before it expires. This eliminates
// reactive 401 retries and makes concurrent operations safe — only the
// refresh itself is serialized.

import type { Storage } from "./storage";
import type {
  MutationResult,
  OpakeInitOptions,
  ResolvedIdentity,
  ResolvedWorkspace,
  WorkspaceEntry,
  WorkspaceMember,
  WorkspaceRole,
  WorkspaceSyncResult,
} from "./types";
import { OpakeError, parseWasmError, wrapWasmErrors } from "./errors";
import {
  resolvedIdentitySchema,
  createWorkspaceResultSchema,
  listWorkspacesResultSchema,
  syncDetailedResultSchema,
} from "./schemas";
import { initWasm } from "./wasm";
import { FileManager } from "./file-manager";
import type { LoginOptions, StartLoginOptions, PendingLogin } from "./auth";
import { createStorageAdapter } from "./storage-adapter";
import {
  createPairRequest as pairingCreate,
  listPairRequests as pairingList,
  listPairResponses as pairingListResponses,
  approvePairRequest as pairingApprove,
  receivePairResponse as pairingReceive,
  cleanupPairRecords as pairingCleanup,
  cleanupExpiredPairRequests as pairingCleanupExpired,
} from "./pairing";

// The WASM module types. We import dynamically after init.
type WasmModule = typeof import("../wasm/opake.js");
type WasmOpakeContext = import("../wasm/opake.js").OpakeContext;

// ---------------------------------------------------------------------------
// Token guard decorator
// ---------------------------------------------------------------------------

const REFRESH_THRESHOLD_MS = 30_000; // refresh 30s before expiry
const PENDING_STORAGE_KEY = "opake:pendingLogin";
const PENDING_TTL_MS = 10 * 60 * 1000; // 10 minutes — generous for a redirect round-trip

/**
 * Method decorator: ensures the OAuth token is valid before each call.
 *
 * If the token expires within the threshold, triggers a single-flight
 * refresh (concurrent callers share the same promise). Eliminates reactive
 * 401 retries and makes concurrent dispatch safe.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any -- TC39 decorator type erasure
function withTokenGuard(_target: any, _context: ClassMethodDecoratorContext) {
  return async function (this: Opake, ...args: any[]): Promise<any> {
    await this.ensureValidToken();
    return _target.call(this, ...args);
  };
}


// ---------------------------------------------------------------------------
// Opake class
// ---------------------------------------------------------------------------

/**
 * The main Opake SDK entry point.
 *
 * Holds an authenticated context backed by a PDS session and encryption
 * identity. Created once via `Opake.init()`, then used for the lifetime
 * of the application.
 *
 * All async methods proactively refresh the OAuth token before expiry —
 * no manual token management needed.
 *
 * @example
 * ```typescript
 * import { Opake } from "@opake/sdk";
 * import { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";
 *
 * const opake = await Opake.init({ storage: new IndexedDbStorage() });
 *
 * const cabinet = opake.cabinet();
 * const tree = await cabinet.loadTree();
 * cabinet.dispose();
 *
 * opake.destroy();
 * ```
 */
export class Opake {
  private ctx: WasmOpakeContext | null;
  private readonly storage: Storage;
  private refreshPromise: Promise<void> | null = null;

  private constructor(ctx: WasmOpakeContext, storage: Storage) {
    this.ctx = ctx;
    this.storage = storage;
  }

  // ---------------------------------------------------------------------------
  // Pre-auth static methods (no Opake instance needed)
  // ---------------------------------------------------------------------------

  /**
   * Check if any accounts exist in storage.
   *
   * Use this before `init()` to decide whether to show a setup/login flow.
   *
   * @example
   * ```typescript
   * if (!(await Opake.isConfigured(storage))) {
   *   showLoginPage();
   * }
   * ```
   */
  static async isConfigured(storage: Storage): Promise<boolean> {
    try {
      const config = await storage.loadConfig();
      return Object.keys(config.accounts).length > 0;
    } catch {
      return false;
    }
  }

  /**
   * List accounts in storage without initializing an Opake instance.
   *
   * @returns Array of `{ did, pdsUrl, handle }` entries.
   */
  static async listAccounts(
    storage: Storage,
  ): Promise<readonly { did: string; pdsUrl: string; handle: string }[]> {
    try {
      const config = await storage.loadConfig();
      return Object.entries(config.accounts).map(([did, entry]) => ({
        did,
        pdsUrl: entry.pds_url,
        handle: entry.handle,
      }));
    } catch {
      return [];
    }
  }

  /** Generate a 24-word BIP-39 seed phrase for identity derivation. */
  static async generateSeedPhrase(): Promise<string> {
    const wasm = await initWasm();
    return wasm.generateMnemonic();
  }

  /** Validate a BIP-39 seed phrase. */
  static async validateSeedPhrase(phrase: string): Promise<boolean> {
    const wasm = await initWasm();
    return wasm.validateMnemonic(phrase);
  }

  /** Derive an encryption identity from a seed phrase. */
  static async createIdentity(
    seedPhrase: string,
    did: string,
  ): Promise<import("./storage").Identity> {
    const wasm = await initWasm();
    return wasm.deriveIdentityFromMnemonic(
      seedPhrase,
      did,
    ) as import("./storage").Identity;
  }

  /** Generate a fresh random encryption identity. */
  static async generateIdentity(
    did: string,
  ): Promise<import("./storage").Identity> {
    const wasm = await initWasm();
    return wasm.generateIdentity(did) as import("./storage").Identity;
  }

  // ---------------------------------------------------------------------------
  // Authentication
  // ---------------------------------------------------------------------------

  /**
   * OAuth 2.0 + DPoP login (one-shot, callback pattern).
   *
   * Handles discovery, PAR, PKCE, DPoP proofs, code exchange, and session
   * storage. The consumer provides an `authorize` callback for the
   * platform-specific redirect step.
   *
   * Does NOT survive page navigations. For full-page redirect flows, use
   * `Opake.startLogin()` / `Opake.completeLogin()`.
   *
   * @example
   * ```typescript
   * // Browser popup
   * await Opake.login("alice.bsky.social", {
   *   storage,
   *   redirectUri: "https://myapp.com/callback",
   *   authorize: async (authUrl) => {
   *     const popup = window.open(authUrl);
   *     return waitForCallbackMessage(popup); // { code, state }
   *   },
   * });
   * const opake = await Opake.init({ storage });
   * ```
   */
  static async login(handle: string, options: LoginOptions): Promise<void> {
    const { authUrl, pending } = await Opake.startLogin(handle, {
      storage: options.storage,
      redirectUri: options.redirectUri,
    });
    const { code, state } = await options.authorize(authUrl);
    await Opake.completeLogin(code, state, pending, {
      storage: options.storage,
      redirectUri: options.redirectUri,
    });
  }

  /**
   * Save pending login state to sessionStorage.
   *
   * Use with `startLogin` / `completeLogin` for redirect flows.
   * `loadPendingLogin` clears the state on read, so the DPoP key material
   * doesn't linger in sessionStorage after the flow completes (or fails).
   */
  static savePendingLogin(pending: PendingLogin): void {
    const envelope = { pending, savedAt: Date.now() };
    sessionStorage.setItem(PENDING_STORAGE_KEY, JSON.stringify(envelope));
  }

  /**
   * Load and clear pending login state from sessionStorage.
   *
   * Returns `null` if no pending state exists or if the state is older
   * than 10 minutes (TTL). Always clears the key — the DPoP key material
   * must not persist regardless of success or failure.
   */
  static loadPendingLogin(): PendingLogin | null {
    const raw = sessionStorage.getItem(PENDING_STORAGE_KEY);
    sessionStorage.removeItem(PENDING_STORAGE_KEY);
    if (!raw) return null;
    try {
      const envelope = JSON.parse(raw) as { pending: PendingLogin; savedAt: number };
      if (Date.now() - envelope.savedAt > PENDING_TTL_MS) return null;
      return envelope.pending;
    } catch {
      return null;
    }
  }

  /**
   * Start an OAuth login flow (two-step, redirect-safe).
   *
   * Returns the auth URL and serializable pending state. Save the pending
   * state with `Opake.savePendingLogin(pending)` before redirecting, then
   * load it with `Opake.loadPendingLogin()` on the callback page.
   *
   * @example
   * ```typescript
   * const { authUrl, pending } = await Opake.startLogin("alice.bsky.social", {
   *   storage,
   *   redirectUri: "https://myapp.com/callback",
   * });
   * Opake.savePendingLogin(pending);
   * window.location.href = authUrl;
   *
   * // ... on callback page:
   * const pending = Opake.loadPendingLogin();
   * const params = new URLSearchParams(window.location.search);
   * await Opake.completeLogin(params.get("code")!, params.get("state")!, pending!, {
   *   storage,
   *   redirectUri: "https://myapp.com/callback",
   * });
   * ```
   */
  static async startLogin(
    handle: string,
    options: StartLoginOptions,
  ): Promise<{ authUrl: string; pending: PendingLogin }> {
    const wasm = await initWasm();
    const adapter = createStorageAdapter(options.storage);
    const result = await wasm.startOAuthLogin(
      handle,
      options.redirectUri,
      adapter,
    );
    return result as { authUrl: string; pending: PendingLogin };
  }

  /**
   * Complete an OAuth login flow after the user returns from authorization.
   *
   * Validates the CSRF state, exchanges the code for tokens with DPoP,
   * and saves the session to storage. Tokens never enter JS memory.
   */
  static async completeLogin(
    code: string,
    state: string,
    pending: PendingLogin,
    options: { storage: Storage; redirectUri: string },
  ): Promise<void> {
    const wasm = await initWasm();
    const adapter = createStorageAdapter(options.storage);
    await wasm.completeOAuthLogin(
      code,
      state,
      pending,
      options.redirectUri,
      adapter,
    );
  }

  /**
   * Login with an app password (legacy createSession).
   *
   * For environments that can't do OAuth redirects — Obsidian plugins,
   * simple scripts, testing. The user creates an app password in their
   * PDS account settings.
   *
   * @example
   * ```typescript
   * await Opake.loginWithAppPassword("alice.bsky.social", "xxxx-xxxx-xxxx-xxxx", { storage });
   * const opake = await Opake.init({ storage });
   * ```
   */
  static async loginWithAppPassword(
    handle: string,
    appPassword: string,
    options: { storage: Storage },
  ): Promise<void> {
    const wasm = await initWasm();
    const adapter = createStorageAdapter(options.storage);
    await wasm.loginWithAppPasswordWasm(handle, appPassword, adapter);
  }

  // ---------------------------------------------------------------------------
  // Initialization
  // ---------------------------------------------------------------------------

  /**
   * Initialize an Opake instance.
   *
   * Bootstraps the WASM module (lazy, cached after first call), then reads
   * config, session, and identity from storage. The resulting instance is
   * long-lived — reuse it across operations.
   *
   * @param options - Storage backend, account DID, WASM URL override.
   * @returns A ready-to-use Opake instance.
   *
   * @example
   * ```typescript
   * const opake = await Opake.init();
   * const opake = await Opake.init({ did: "did:plc:abc123" });
   * ```
   */
  static async init(options?: OpakeInitOptions): Promise<Opake> {
    const wasm: WasmModule = await initWasm(options?.wasmUrl);

    let storage: Storage;
    if (options?.storage) {
      storage = options.storage;
    } else {
      const { IndexedDbStorage } = await import("./storage/indexeddb");
      storage = new IndexedDbStorage();
    }

    const adapter = createStorageAdapter(storage);

    try {
      const ctx = await wasm.OpakeContext.create(options?.did ?? null, adapter);
      return new Opake(ctx, storage);
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  // ---------------------------------------------------------------------------
  // Session validation
  // ---------------------------------------------------------------------------

  /**
   * Verify the session is usable by touching the account config record.
   *
   * Reads the config, stamps `modifiedAt` with the current time, and
   * writes it back via `putRecord` (authenticated). Throws on auth
   * failure — use during boot to detect dead sessions.
   */
  @wrapWasmErrors
  async checkSession(): Promise<void> {
    await this.requireContext().checkSession();
  }

  // ---------------------------------------------------------------------------
  // Token lifecycle (called by @withTokenGuard decorator)
  // ---------------------------------------------------------------------------

  /**
   * Ensure the current OAuth token is valid, refreshing proactively if needed.
   *
   * Single-flight: concurrent callers share one refresh promise. This is
   * the only serialization point — all other operations can run in parallel.
   *
   * @internal — called by the @withTokenGuard decorator, not by consumers.
   */
  async ensureValidToken(): Promise<void> {
    const ctx = this.requireContext();

    // tokenExpiresAt returns only the timestamp — no tokens cross to JS.
    const expiresAt = ctx.tokenExpiresAt();
    if (expiresAt < 0) return; // legacy session or unknown — skip proactive refresh

    const expiresMs = expiresAt * 1000;
    if (Date.now() + REFRESH_THRESHOLD_MS < expiresMs) return;

    // Token expiring soon — deduplicated refresh
    this.refreshPromise ??= this.doRefresh().finally(() => {
      this.refreshPromise = null;
    });
    await this.refreshPromise;
  }

  private async doRefresh(): Promise<void> {
    const ctx = this.requireContext();
    try {
      await ctx.proactiveRefresh();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.warn("opake: proactive token refresh failed:", msg);
    }
  }

  // ---------------------------------------------------------------------------
  // Group key cache
  // ---------------------------------------------------------------------------

  // ---------------------------------------------------------------------------
  // FileManager creation
  // ---------------------------------------------------------------------------

  /**
   * Create a FileManager for personal (cabinet) file operations.
   *
   * The cabinet is your private file space — files encrypted to your own
   * identity. Call `.dispose()` when done to release the context for reuse.
   *
   * @example
   * ```typescript
   * const cabinet = opake.cabinet();
   * await cabinet.upload(data, "photo.jpg", "image/jpeg");
   * cabinet.dispose();
   * ```
   */
  @wrapWasmErrors
  cabinet(): FileManager {
    return new FileManager(this.requireContext().cabinet());
  }

  /**
   * Resolve a workspace by keyring URI and create a FileManager.
   *
   * Fetches the keyring record, unwraps the group key using your identity,
   * and returns a ready-to-use FileManager. The group key never leaves WASM.
   *
   * Works for both your own workspaces and workspaces where you're a member
   * on someone else's PDS.
   *
   * @param keyringUri - AT URI of the workspace's keyring record.
   *
   * @example
   * ```typescript
   * const workspaces = await opake.listWorkspaces();
   * const ws = await opake.workspace(workspaces[0].uri);
   * await ws.upload(data, "report.pdf", "application/pdf");
   * ws.dispose();
   * ```
   */
  @wrapWasmErrors @withTokenGuard
  async workspace(keyringUri: string): Promise<FileManager> {
    return new FileManager(await this.requireContext().workspaceByUri(keyringUri));
  }

  /**
   * Create a FileManager for a workspace from already-resolved key material.
   *
   * Advanced use — prefer `workspace(keyringUri)` which resolves internally
   * and keeps the group key inside WASM.
   *
   * @param workspace - Resolved workspace context with key material.
   */
  @wrapWasmErrors
  workspaceFromKey(workspace: ResolvedWorkspace): FileManager {
    const ctx = this.requireContext();
    return new FileManager(ctx.workspace(workspace.keyringUri, workspace.ownerDid, workspace.key, BigInt(workspace.rotation)));
  }

  // ---------------------------------------------------------------------------
  // Workspace management
  // ---------------------------------------------------------------------------

  /**
   * Create a new workspace (keyring).
   *
   * @returns The keyring URI and group key for the new workspace.
   *
   * @example
   * ```typescript
   * const { keyringUri, key } = await opake.createWorkspace("family-photos");
   * ```
   */
  @wrapWasmErrors @withTokenGuard
  createWorkspace(name: string, description?: string): Promise<{ keyringUri: string; key: Uint8Array }> {
    return this.requireContext().createWorkspace(name, description ?? null).then(createWorkspaceResultSchema.parse);
  }

  /**
   * List all workspaces the current user is a member of.
   *
   * Also populates the in-memory group key cache for subsequent
   * `workspaceFromKey()` calls.
   *
   * @returns Array of workspace entries with decrypted names and roles.
   */
  @wrapWasmErrors @withTokenGuard
  listWorkspaces(): Promise<readonly WorkspaceEntry[]> {
    return this.requireContext().listWorkspaces(null).then(listWorkspacesResultSchema.parse);
  }

  /**
   * List members of a workspace.
   *
   * @param keyringUri - Workspace keyring URI.
   * @returns Array of keyring member records with DIDs and roles.
   */
  @wrapWasmErrors @withTokenGuard
  listWorkspaceMembers(keyringUri: string): Promise<readonly WorkspaceMember[]> {
    return this.requireContext().listWorkspaceMembers(keyringUri) as Promise<readonly WorkspaceMember[]>;
  }

  /**
   * Add a member to a workspace.
   */
  @wrapWasmErrors @withTokenGuard
  addWorkspaceMember(
    keyringUri: string, key: Uint8Array, memberDid: string, memberPublicKey: Uint8Array, role: WorkspaceRole,
  ): Promise<MutationResult> {
    return this.requireContext().addWorkspaceMember(keyringUri, key, memberDid, memberPublicKey, role) as Promise<MutationResult>;
  }

  /**
   * Remove a member from a workspace.
   *
   * For owners: rotates the group key and returns the new key + rotation.
   * For non-owners: creates a proposal.
   */
  @wrapWasmErrors @withTokenGuard
  removeWorkspaceMember(
    keyringUri: string, key: Uint8Array, memberDid: string,
  ): Promise<{ key?: Uint8Array; rotation?: number; proposed: boolean }> {
    return this.requireContext().removeWorkspaceMember(keyringUri, key, memberDid) as Promise<{ key?: Uint8Array; rotation?: number; proposed: boolean }>;
  }

  /** Leave a workspace you're a member of. */
  @wrapWasmErrors @withTokenGuard
  leaveWorkspace(keyringUri: string): Promise<string> {
    return this.requireContext().leaveWorkspace(keyringUri);
  }

  /** Update workspace metadata (name, description, icon). */
  @wrapWasmErrors @withTokenGuard
  updateWorkspaceMetadata(
    keyringUri: string, key: Uint8Array, updates: { name?: string; description?: string; icon?: string },
  ): Promise<MutationResult> {
    return this.requireContext().updateWorkspaceMetadata(
      keyringUri, key, updates.name ?? null, updates.description ?? null, updates.icon ?? null,
    ) as Promise<MutationResult>;
  }

  /** Update a workspace member's role. */
  @wrapWasmErrors @withTokenGuard
  updateMemberRole(keyringUri: string, memberDid: string, role: WorkspaceRole): Promise<MutationResult> {
    return this.requireContext().updateMemberRole(keyringUri, memberDid, role) as Promise<MutationResult>;
  }

  // ---------------------------------------------------------------------------
  // Identity & account
  // ---------------------------------------------------------------------------

  /**
   * Resolve a user's identity (handle or DID → DID, PDS URL, public key).
   *
   * @param handleOrDid - AT Protocol handle or DID.
   */
  @wrapWasmErrors @withTokenGuard
  resolveIdentity(handleOrDid: string): Promise<ResolvedIdentity> {
    return this.requireContext().resolveIdentity(handleOrDid).then(resolvedIdentitySchema.parse);
  }

  /**
   * Publish the current identity's public encryption key to the PDS.
   *
   * Required before other users can encrypt files for you or add you
   * to workspaces.
   */
  @wrapWasmErrors @withTokenGuard
  publishPublicKey(): Promise<string> {
    return this.requireContext().publishPublicKey();
  }

  // ---------------------------------------------------------------------------
  // Daemon operations
  // ---------------------------------------------------------------------------

  /** Sync all owned workspaces — apply pending proposals from members. */
  @wrapWasmErrors @withTokenGuard
  syncOwnedWorkspaces(): Promise<number> {
    return this.requireContext().syncOwnedWorkspaces();
  }

  /** Sync with per-workspace result visibility (daemon use). */
  @wrapWasmErrors @withTokenGuard
  syncOwnedWorkspacesDetailed(): Promise<readonly WorkspaceSyncResult[]> {
    return this.requireContext().syncOwnedWorkspacesDetailed().then(syncDetailedResultSchema.parse);
  }

  /** Retry pending shares — resolve recipients and create grants. */
  @wrapWasmErrors @withTokenGuard
  retryPendingShares(): Promise<{ checked: number; completed: number; expired: number; still_pending: number; failed: number }> {
    return this.requireContext().retryPendingSharesViaOpake();
  }

  // ---------------------------------------------------------------------------
  // Device pairing (implementations in ./pairing.ts)
  // ---------------------------------------------------------------------------

  /** Create a pair request (new device). Returns the record URI + ephemeral keypair. */
  @wrapWasmErrors @withTokenGuard
  createPairRequest(): Promise<import("./types").PairRequestResult> {
    return pairingCreate(this.requireContext());
  }

  /** List pending pair requests on this account. */
  @wrapWasmErrors @withTokenGuard
  listPairRequests(): Promise<readonly import("./types").PendingPairRequest[]> {
    return pairingList(this.requireContext());
  }

  /** List pair responses on this account. */
  @wrapWasmErrors @withTokenGuard
  listPairResponses(): Promise<readonly { uri: string; requestUri: string; value: import("./types").PairResponseRecord }[]> {
    return pairingListResponses(this.requireContext());
  }

  /** Approve a pair request (existing device). Encrypts and sends the identity. */
  @wrapWasmErrors @withTokenGuard
  approvePairRequest(requestUri: string, ephemeralPublicKey: Uint8Array): Promise<void> {
    return pairingApprove(this.requireContext(), requestUri, ephemeralPublicKey);
  }

  /** Receive a pair response (new device). Decrypts the identity from the approving device. */
  @wrapWasmErrors @withTokenGuard
  receivePairResponse(response: import("./types").PairResponseRecord, ephemeralPrivateKey: Uint8Array): Promise<import("./storage").Identity> {
    return pairingReceive(this.requireContext(), response, ephemeralPrivateKey);
  }

  /** Clean up pair request + response records after successful pairing. */
  @wrapWasmErrors @withTokenGuard
  cleanupPairRecords(requestRkey: string, responseRkey: string): Promise<void> {
    return pairingCleanup(this.requireContext(), requestRkey, responseRkey);
  }

  /** Delete expired pair requests and orphaned responses. */
  @wrapWasmErrors @withTokenGuard
  cleanupExpiredPairRequests(): Promise<number> {
    return pairingCleanupExpired(this.requireContext());
  }

  /** Delete stale grants whose recipients have no valid public key. */
  @wrapWasmErrors @withTokenGuard
  healStaleGrants(): Promise<number> {
    return this.requireContext().healStaleGrants();
  }

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

  /**
   * Free the underlying WASM context and clear the key cache.
   *
   * After calling `destroy()`, all methods will throw.
   */
  destroy(): void {
    if (this.ctx) {
      this.ctx.free();
      this.ctx = null;
    }
  }

  /** @internal */
  private requireContext(): WasmOpakeContext {
    if (!this.ctx) {
      throw new OpakeError("Unknown", "Opake instance has been destroyed");
    }
    return this.ctx;
  }

}
