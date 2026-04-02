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

import type { Storage, OAuthSession } from "./storage";
import type {
  MutationResult,
  OpakeInitOptions,
  ResolvedIdentity,
  ResolvedWorkspace,
  WorkspaceEntry,
  WorkspaceRole,
  WorkspaceSyncResult,
} from "./types";
import { OpakeError, parseWasmError } from "./errors";
import { initWasm } from "./wasm";
import { FileManager } from "./file-manager";
import {
  login as authLogin,
  loginWithAppPassword as authLoginWithAppPassword,
  startLogin as authStartLogin,
  completeLogin as authCompleteLogin,
  type LoginOptions,
  type StartLoginOptions,
  type PendingLogin,
} from "./auth";

// The WASM module types. We import dynamically after init.
type WasmModule = typeof import("../wasm/opake.js");
type WasmOpakeContext = InstanceType<WasmModule["OpakeContext"]>;

// ---------------------------------------------------------------------------
// Token guard decorator
// ---------------------------------------------------------------------------

const REFRESH_THRESHOLD_MS = 30_000; // refresh 30s before expiry

/**
 * Method decorator: ensures the OAuth token is valid before each call.
 *
 * If the token expires within the threshold, triggers a single-flight
 * refresh (concurrent callers share the same promise). Eliminates reactive
 * 401 retries and makes concurrent dispatch safe.
 */
function withTokenGuard<T extends (...args: never[]) => Promise<unknown>>(
  target: T,
  _context: ClassMethodDecoratorContext<Opake>,
): T {
  async function guarded(this: Opake, ...args: unknown[]): Promise<unknown> {
    await this.ensureValidToken();
    return (target as (...a: unknown[]) => Promise<unknown>).call(this, ...args);
  }
  return guarded as unknown as T;
}

// ---------------------------------------------------------------------------
// Storage adapter bridge
// ---------------------------------------------------------------------------

/**
 * Create the storage adapter object that the WASM JsStorageAdapter expects.
 */
function createStorageAdapter(storage: Storage): Record<string, unknown> {
  return {
    loadConfig: () => storage.loadConfig(),
    saveConfig: (config: unknown) => storage.saveConfig(config as Parameters<Storage["saveConfig"]>[0]),
    loadIdentity: (did: string) => storage.loadIdentity(did),
    saveIdentity: (did: string, identity: unknown) =>
      storage.saveIdentity(did, identity as Parameters<Storage["saveIdentity"]>[1]),
    loadSession: (did: string) => storage.loadSession(did),
    saveSession: (did: string, session: unknown) =>
      storage.saveSession(did, session as Parameters<Storage["saveSession"]>[1]),
    removeAccount: (did: string) => storage.removeAccount(did),
    cacheGetRecord: (did: string, collection: string, uri: string) =>
      storage.cacheGetRecord(did, collection, uri),
    cachePutRecords: (did: string, collection: string, records: unknown) =>
      storage.cachePutRecords(did, collection, records as Parameters<Storage["cachePutRecords"]>[2]),
    cacheRemoveRecord: (did: string, collection: string, uri: string) =>
      storage.cacheRemoveRecord(did, collection, uri),
    cacheGetCollection: (did: string, collection: string) =>
      storage.cacheGetCollection(did, collection),
    cachePutCollection: (did: string, collection: string, data: unknown) =>
      storage.cachePutCollection(
        did,
        collection,
        data as Parameters<Storage["cachePutCollection"]>[2],
      ),
    cacheInvalidateCollection: (did: string, collection: string) =>
      storage.cacheInvalidateCollection(did, collection),
    cacheClear: (did: string) => storage.cacheClear(did),
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
    return wasm.deriveIdentityFromMnemonic(seedPhrase, did) as import("./storage").Identity;
  }

  /** Generate a fresh random encryption identity. */
  static async generateIdentity(did: string): Promise<import("./storage").Identity> {
    const wasm = await initWasm();
    return wasm.generateIdentity(did) as import("./storage").Identity;
  }

  /** Generate a DPoP keypair for OAuth token binding. */
  static async generateDpopKeyPair(): Promise<import("./storage").DpopKeyPair> {
    const wasm = await initWasm();
    return wasm.generateDpopKeyPair() as import("./storage").DpopKeyPair;
  }

  /** Create a DPoP proof JWT for an OAuth request. */
  static async createDpopProof(
    keypair: import("./storage").DpopKeyPair,
    method: string,
    url: string,
    timestamp: number,
    nonce?: string,
    token?: string,
  ): Promise<string> {
    const wasm = await initWasm();
    return wasm.createDpopProof(keypair, method, url, timestamp, nonce ?? null, token ?? null);
  }

  /** Generate a PKCE challenge for OAuth authorization. */
  static async generatePkce(): Promise<{ verifier: string; challenge: string }> {
    const wasm = await initWasm();
    return wasm.generatePkce() as { verifier: string; challenge: string };
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
    return authLogin(handle, options);
  }

  /**
   * Start an OAuth login flow (two-step, redirect-safe).
   *
   * Returns the auth URL and serializable pending state. The consumer
   * saves `pending` to sessionStorage, redirects the user, then calls
   * `Opake.completeLogin()` with the callback parameters.
   *
   * @example
   * ```typescript
   * const { authUrl, pending } = await Opake.startLogin("alice.bsky.social", {
   *   storage,
   *   redirectUri: "https://myapp.com/callback",
   * });
   * sessionStorage.setItem("opake:pending", JSON.stringify(pending));
   * window.location.href = authUrl;
   *
   * // ... on callback page:
   * const pending = JSON.parse(sessionStorage.getItem("opake:pending")!);
   * const params = new URLSearchParams(window.location.search);
   * await Opake.completeLogin(params.get("code")!, params.get("state")!, pending, {
   *   storage,
   *   redirectUri: "https://myapp.com/callback",
   * });
   * ```
   */
  static async startLogin(
    handle: string,
    options: StartLoginOptions,
  ): Promise<{ authUrl: string; pending: PendingLogin }> {
    return authStartLogin(handle, options);
  }

  /**
   * Complete an OAuth login flow after the user returns from authorization.
   *
   * Validates the CSRF state, exchanges the code for tokens with DPoP,
   * and saves the session to storage.
   */
  static async completeLogin(
    code: string,
    state: string,
    pending: PendingLogin,
    options: { storage: Storage; redirectUri: string },
  ): Promise<void> {
    return authCompleteLogin(code, state, pending, options);
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
    return authLoginWithAppPassword(handle, appPassword, options);
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
    try {
      const sessionValue = ctx.session();
      const session = sessionValue as OAuthSession | null;
      if (!session || session.type !== "oauth" || !session.expires_at) return;

      const expiresMs = session.expires_at * 1000;
      if (Date.now() + REFRESH_THRESHOLD_MS < expiresMs) return;
    } catch {
      // Can't read session — let the actual call fail with a proper error
      return;
    }

    // Token expiring soon — deduplicated refresh
    this.refreshPromise ??= this.doRefresh().finally(() => {
      this.refreshPromise = null;
    });
    await this.refreshPromise;
  }

  private async doRefresh(): Promise<void> {
    // The WASM XRPC client handles reactive refresh internally (on 401).
    // Proactive refresh: we make a lightweight call that triggers signoff,
    // which auto-persists the refreshed session to storage.
    // getAccountConfig is cheap and touches the PDS, triggering refresh.
    const ctx = this.requireContext();
    try {
      await ctx.getAccountConfig();
    } catch {
      // Best effort — if this fails, the next real call will trigger
      // reactive refresh via the XRPC client's 401 handler.
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
  cabinet(): FileManager {
    const ctx = this.requireContext();
    try {
      return new FileManager(ctx.cabinet());
    } catch (e) {
      throw parseWasmError(e);
    }
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
  @withTokenGuard
  async workspace(keyringUri: string): Promise<FileManager> {
    const ctx = this.requireContext();
    try {
      const handle = await ctx.workspaceByUri(keyringUri);
      return new FileManager(handle);
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * Create a FileManager for a workspace from already-resolved key material.
   *
   * Advanced use — prefer `workspace(keyringUri)` which resolves internally
   * and keeps the group key inside WASM.
   *
   * @param workspace - Resolved workspace context with key material.
   */
  workspaceFromKey(workspace: ResolvedWorkspace): FileManager {
    const ctx = this.requireContext();
    try {
      return new FileManager(
        ctx.workspace(workspace.keyringUri, workspace.ownerDid, workspace.key, BigInt(workspace.rotation)),
      );
    } catch (e) {
      throw parseWasmError(e);
    }
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
  @withTokenGuard
  async createWorkspace(name: string, description?: string): Promise<{ keyringUri: string; key: Uint8Array }> {
    const ctx = this.requireContext();
    try {
      const result = await ctx.createWorkspace(name, description ?? null);
      const parsed = result as { keyring_uri: string; key: Uint8Array };
      return { keyringUri: parsed.keyring_uri, key: parsed.key };
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * List all workspaces the current user is a member of.
   *
   * Also populates the in-memory group key cache for subsequent
   * `workspaceFromKey()` calls.
   *
   * @returns Array of workspace entries with decrypted names and roles.
   */
  @withTokenGuard
  async listWorkspaces(): Promise<readonly WorkspaceEntry[]> {
    const ctx = this.requireContext();
    try {
      const result = await ctx.listWorkspaces(null);
      const parsed = result as { keyrings: readonly WorkspaceEntry[] };
      return parsed.keyrings;
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * List members of a workspace.
   *
   * @param keyringUri - Workspace keyring URI.
   * @returns Array of keyring member records with DIDs and roles.
   */
  @withTokenGuard
  async listWorkspaceMembers(keyringUri: string): Promise<unknown> {
    const ctx = this.requireContext();
    try {
      return await ctx.listWorkspaceMembers(keyringUri);
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * Add a member to a workspace.
   */
  @withTokenGuard
  async addWorkspaceMember(
    keyringUri: string,
    key: Uint8Array,
    memberDid: string,
    memberPublicKey: Uint8Array,
    role: WorkspaceRole,
  ): Promise<MutationResult> {
    const ctx = this.requireContext();
    try {
      const result = await ctx.addWorkspaceMember(keyringUri, key, memberDid, memberPublicKey, role);
      return result as MutationResult;
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * Remove a member from a workspace.
   *
   * For owners: rotates the group key and returns the new key + rotation.
   * For non-owners: creates a proposal.
   */
  @withTokenGuard
  async removeWorkspaceMember(
    keyringUri: string,
    key: Uint8Array,
    memberDid: string,
  ): Promise<{ key?: Uint8Array; rotation?: number; proposed: boolean }> {
    const ctx = this.requireContext();
    try {
      const result = await ctx.removeWorkspaceMember(keyringUri, key, memberDid);
      return result as { key?: Uint8Array; rotation?: number; proposed: boolean };
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /** Leave a workspace you're a member of. */
  @withTokenGuard
  async leaveWorkspace(keyringUri: string): Promise<string> {
    const ctx = this.requireContext();
    try {
      return await ctx.leaveWorkspace(keyringUri);
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /** Update workspace metadata (name, description, icon). */
  @withTokenGuard
  async updateWorkspaceMetadata(
    keyringUri: string,
    key: Uint8Array,
    updates: { name?: string; description?: string; icon?: string },
  ): Promise<MutationResult> {
    const ctx = this.requireContext();
    try {
      const result = await ctx.updateWorkspaceMetadata(
        keyringUri, key, updates.name ?? null, updates.description ?? null, updates.icon ?? null,
      );
      return result as MutationResult;
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /** Update a workspace member's role. */
  @withTokenGuard
  async updateMemberRole(keyringUri: string, memberDid: string, role: WorkspaceRole): Promise<MutationResult> {
    const ctx = this.requireContext();
    try {
      const result = await ctx.updateMemberRole(keyringUri, memberDid, role);
      return result as MutationResult;
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  // ---------------------------------------------------------------------------
  // Identity & account
  // ---------------------------------------------------------------------------

  /**
   * Resolve a user's identity (handle or DID → DID, PDS URL, public key).
   *
   * @param handleOrDid - AT Protocol handle or DID.
   */
  @withTokenGuard
  async resolveIdentity(handleOrDid: string): Promise<ResolvedIdentity> {
    const ctx = this.requireContext();
    try {
      return await ctx.resolveIdentity(handleOrDid) as ResolvedIdentity;
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * Publish the current identity's public encryption key to the PDS.
   *
   * Required before other users can encrypt files for you or add you
   * to workspaces.
   */
  @withTokenGuard
  async publishPublicKey(): Promise<string> {
    const ctx = this.requireContext();
    try {
      return await ctx.publishPublicKey();
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  // ---------------------------------------------------------------------------
  // Daemon operations
  // ---------------------------------------------------------------------------

  /** Sync all owned workspaces — apply pending proposals from members. */
  @withTokenGuard
  async syncOwnedWorkspaces(): Promise<number> {
    const ctx = this.requireContext();
    try {
      return await ctx.syncOwnedWorkspaces();
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /** Sync with per-workspace result visibility (daemon use). */
  @withTokenGuard
  async syncOwnedWorkspacesDetailed(): Promise<readonly WorkspaceSyncResult[]> {
    const ctx = this.requireContext();
    try {
      const result = await ctx.syncOwnedWorkspacesDetailed();
      const parsed = result as {
        workspaces: readonly { keyring_uri: string; proposals_applied: number; error?: string }[];
      };
      // Map snake_case wire format to camelCase public API
      return parsed.workspaces.map((r) => ({
        keyringUri: r.keyring_uri,
        proposalsApplied: r.proposals_applied,
        error: r.error,
      }));
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /** Retry pending shares — resolve recipients and create grants. */
  @withTokenGuard
  async retryPendingShares(): Promise<{
    checked: number; completed: number; expired: number; still_pending: number; failed: number;
  }> {
    const ctx = this.requireContext();
    try {
      return await ctx.retryPendingSharesViaOpake() as {
        checked: number; completed: number; expired: number; still_pending: number; failed: number;
      };
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /** Delete expired pair requests and orphaned responses. */
  @withTokenGuard
  async cleanupExpiredPairRequests(): Promise<number> {
    const ctx = this.requireContext();
    try {
      return await ctx.cleanupExpiredPairRequests();
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /** Delete stale grants whose recipients have no valid public key. */
  @withTokenGuard
  async healStaleGrants(): Promise<number> {
    const ctx = this.requireContext();
    try {
      return await ctx.healStaleGrants();
    } catch (e) {
      throw parseWasmError(e);
    }
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
