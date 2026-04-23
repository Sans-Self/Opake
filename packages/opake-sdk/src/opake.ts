// Opake — the main entry point for the SDK.
//
// Holds a long-lived WasmOpakeHandle (authenticated XRPC client + identity +
// storage). Create via `Opake.init()`, then call `.cabinet()` or
// `.workspace(keyringUri)` to get a FileManager for file operations.
//
// All public async methods are decorated with @withTokenGuard, which
// proactively refreshes the OAuth token before it expires. This eliminates
// reactive 401 retries and makes concurrent operations safe — only the
// refresh itself is serialized.

import type { Storage } from "./storage";
import type {
  AccountConfig,
  AccountConfigPatch,
  DownloadResult,
  InboxGrant,
  InboxSnapshot,
  InboxWatcher,
  MutationResult,
  OpakeInitOptions,
  PendingShareEntry,
  ResolvedGrantMetadata,
  ResolvedIdentity,
  WorkspaceEntry,
  WorkspaceMember,
  WorkspaceRole,
  WorkspaceSyncResult,
} from "./types";
import { OpakeError, parseWasmError, wrapWasmErrors } from "./errors";
import {
  resolvedIdentitySchema,
  createWorkspaceResultSchema,
  downloadResultSchema,
  inboxGrantsSchema,
  inboxSnapshotSchema,
  listWorkspacesResultSchema,
  pendingShareEntriesSchema,
  resolvedGrantMetadataSchema,
  syncSingleResultSchema,
  workspaceSnapshotSchema,
  type WorkspaceSnapshot,
} from "./schemas";
import { initWasm } from "./wasm";
import { FileManager } from "./file-manager";
import type { LoginOptions, StartLoginOptions, PendingLogin } from "./auth";
import { createStorageAdapter } from "./storage-adapter";
import { registerCleanup, unregisterCleanup } from "./finalizer";
import {
  createPairRequest as pairingCreate,
  awaitPairCompletion as pairingAwait,
  cancelPairRequest as pairingCancel,
  listPairRequests as pairingList,
  approvePairRequest as pairingApprove,
  cleanupExpiredPairRequests as pairingCleanupExpired,
  type AwaitPairOptions,
} from "./pairing";

// The WASM module types. We import dynamically after init.
type WasmModule = typeof import("../wasm/opake.js");
type WasmOpakeContext = import("../wasm/opake.js").OpakeContext;

/** Internal shape of the WASM `WorkspaceWatcher` object. */
type WasmWorkspaceWatcherHandle = {
  close(): Promise<void>;
  free(): void;
};

/** Internal shape of the WASM `InboxWatcher` object. */
type WasmInboxWatcherHandle = {
  close(): Promise<void>;
  free(): void;
};

/**
 * Handle returned by `Opake.watchWorkspaces`. Call `.close()` to
 * unsubscribe — typically from a React useEffect cleanup.
 */
export interface WorkspaceWatcher {
  /** Stop receiving notifications. Idempotent. */
  close(): void;
}

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
 * The signed-in DID is available on the instance as `opake.did` —
 * invariant for the instance's lifetime, populated at init time.
 *
 * @example
 * ```typescript
 * import { Opake } from "@opake/sdk";
 * import { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";
 *
 * const opake = await Opake.init({ storage: new IndexedDbStorage() });
 * console.log(`signed in as ${opake.did}`);
 *
 * const cabinet = await opake.cabinet();
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

  /**
   * The DID this Opake was constructed for.
   *
   * Populated once in `Opake.init()` from the underlying WASM context.
   * Invariant for the lifetime of the instance — if you need to switch
   * accounts, destroy this Opake and `init()` a new one with the target
   * DID. Useful for rendering "signed in as..." UI and for keying
   * consumer-side caches by account.
   */
  readonly did: string;

  private constructor(ctx: WasmOpakeContext, storage: Storage, did: string) {
    this.ctx = ctx;
    this.storage = storage;
    this.did = did;
    registerCleanup(this, ctx, this);
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
    const result = await wasm.startOAuthLogin(handle, options.redirectUri, adapter);
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
    await wasm.completeOAuthLogin(code, state, pending, options.redirectUri, adapter);
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
      // Cache the resolved DID — sync read, no lock contention possible
      // this early in the lifecycle (nothing else holds the WASM mutex yet).
      const did = ctx.getDid();
      return new Opake(ctx, storage, did);
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  // ---------------------------------------------------------------------------
  // Session validation
  // ---------------------------------------------------------------------------

  /**
   * Override the cached indexer URL at runtime.
   *
   * `Opake.init` seeds the instance with the compile-time
   * `DEFAULT_INDEXER_URL` baked into the WASM binary. Call this at boot
   * to inject a host-specific runtime default (e.g. the web app's
   * `VITE_INDEXER_URL`, which can't be baked in because one WASM binary
   * is shipped to multiple deployments).
   *
   * After this call, methods that resolve the indexer URL internally
   * (`listWorkspaces`, `startSseConsumer`, etc.) pick up the new value
   * automatically — JS callers don't pass the URL at the call site.
   *
   * Writes to `accountConfig` on the PDS still override this value, so
   * a user-configured indexer (from settings) wins over the host default.
   */
  @wrapWasmErrors
  async setIndexerUrl(url: string): Promise<void> {
    await this.requireContext().setIndexerUrl(url);
  }

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
   * const cabinet = await opake.cabinet();
   * await cabinet.upload(data, "photo.jpg", "image/jpeg");
   * cabinet.dispose();
   * ```
   */
  @wrapWasmErrors
  @withTokenGuard
  async cabinet(): Promise<FileManager> {
    const ctx = this.requireContext();
    const handle = await ctx.cabinet();
    return new FileManager(handle);
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
  @wrapWasmErrors
  @withTokenGuard
  async workspace(keyringUri: string): Promise<FileManager> {
    return new FileManager(await this.requireContext().workspaceByUri(keyringUri));
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
  @wrapWasmErrors
  @withTokenGuard
  createWorkspace(
    name: string,
    description?: string,
  ): Promise<{ keyringUri: string; key: Uint8Array }> {
    return this.requireContext()
      .createWorkspace(name, description ?? null)
      .then(createWorkspaceResultSchema.parse);
  }

  /**
   * List all workspaces the current user is a member of.
   *
   * Also bootstraps the in-memory `WorkspaceKeeper` — `watchWorkspaces`
   * callers see a fresh snapshot with `loaded = true` as a side effect.
   *
   * The indexer URL is resolved internally from the stored config
   * (set during `init` and overridable via `setIndexerUrl` or
   * by writing an `accountConfig` record). Callers do not pass it.
   *
   * @returns Array of workspace entries with decrypted names and roles.
   */
  @wrapWasmErrors
  @withTokenGuard
  listWorkspaces(): Promise<readonly WorkspaceEntry[]> {
    return this.requireContext()
      .listWorkspaces()
      .then(listWorkspacesResultSchema.parse);
  }

  /**
   * List members of a workspace.
   *
   * @param keyringUri - Workspace keyring URI.
   * @returns Array of keyring member records with DIDs and roles.
   */
  @wrapWasmErrors
  @withTokenGuard
  listWorkspaceMembers(keyringUri: string): Promise<readonly WorkspaceMember[]> {
    return this.requireContext().listWorkspaceMembers(keyringUri) as Promise<
      readonly WorkspaceMember[]
    >;
  }

  /**
   * Add a member to a workspace. The WASM binding resolves the keyring
   * and unwraps the group key internally — the key never crosses into JS.
   */
  @wrapWasmErrors
  @withTokenGuard
  addWorkspaceMember(
    keyringUri: string,
    memberDid: string,
    memberPublicKey: Uint8Array,
    role: WorkspaceRole,
  ): Promise<MutationResult> {
    return this.requireContext().addWorkspaceMember(
      keyringUri,
      memberDid,
      memberPublicKey,
      role,
    ) as Promise<MutationResult>;
  }

  /**
   * Remove a member from a workspace.
   *
   * For owners: rotates the group key in-place inside WASM and returns
   * the new `rotation` number. For non-owners: creates a proposal. The
   * rotated key bytes never cross into JS — the next workspace operation
   * re-resolves via the keyring URI.
   */
  @wrapWasmErrors
  @withTokenGuard
  removeWorkspaceMember(
    keyringUri: string,
    memberDid: string,
  ): Promise<{ rotation?: number; proposed: boolean }> {
    return this.requireContext().removeWorkspaceMember(keyringUri, memberDid) as Promise<{
      rotation?: number;
      proposed: boolean;
    }>;
  }

  /** Leave a workspace you're a member of. */
  @wrapWasmErrors
  @withTokenGuard
  leaveWorkspace(keyringUri: string): Promise<string> {
    return this.requireContext().leaveWorkspace(keyringUri);
  }

  /**
   * Update workspace metadata (name, description, icon). The WASM binding
   * resolves the keyring and unwraps the group key internally — the key
   * never crosses into JS.
   */
  @wrapWasmErrors
  @withTokenGuard
  updateWorkspaceMetadata(
    keyringUri: string,
    updates: { name?: string; description?: string; icon?: string },
  ): Promise<MutationResult> {
    return this.requireContext().updateWorkspaceMetadata(
      keyringUri,
      updates.name ?? null,
      updates.description ?? null,
      updates.icon ?? null,
    ) as Promise<MutationResult>;
  }

  /** Update a workspace member's role. */
  @wrapWasmErrors
  @withTokenGuard
  updateMemberRole(
    keyringUri: string,
    memberDid: string,
    role: WorkspaceRole,
  ): Promise<MutationResult> {
    return this.requireContext().updateMemberRole(
      keyringUri,
      memberDid,
      role,
    ) as Promise<MutationResult>;
  }

  // ---------------------------------------------------------------------------
  // Identity & account
  // ---------------------------------------------------------------------------

  /**
   * Resolve a user's identity (handle or DID → DID, PDS URL, public key).
   *
   * @param handleOrDid - AT Protocol handle or DID.
   */
  @wrapWasmErrors
  @withTokenGuard
  resolveIdentity(handleOrDid: string): Promise<ResolvedIdentity> {
    return this.requireContext().resolveIdentity(handleOrDid).then(resolvedIdentitySchema.parse);
  }

  /**
   * Publish the current identity's public encryption key to the PDS.
   *
   * Required before other users can encrypt files for you or add you
   * to workspaces.
   */
  @wrapWasmErrors
  @withTokenGuard
  publishPublicKey(): Promise<string> {
    return this.requireContext().publishPublicKey();
  }

  // ---------------------------------------------------------------------------
  // Account config (per-account preferences synced to PDS)
  // ---------------------------------------------------------------------------

  /**
   * Fetch the account config record (`app.opake.accountConfig/self`), if
   * one exists on the PDS. Returns null when the account has never
   * written a config.
   */
  @wrapWasmErrors
  @withTokenGuard
  getAccountConfig(): Promise<AccountConfig | null> {
    return this.requireContext().getAccountConfig() as Promise<AccountConfig | null>;
  }

  /**
   * Patch the account config record. Read-merge-write happens in core
   * under a single mutex — concurrent calls serialize rather than
   * clobbering each other.
   *
   * Tri-state semantics (see `AccountConfigPatch`):
   * - absent key / `undefined` → field unchanged on the PDS.
   * - `null` (`indexerUrl` only) → field cleared on the PDS.
   * - concrete value → field updated to that value.
   *
   * @returns The freshly-written record.
   */
  @wrapWasmErrors
  @withTokenGuard
  async updateAccountConfig(updates: AccountConfigPatch): Promise<AccountConfig> {
    const ctx = this.requireContext();
    // WASM AccountConfigUpdates uses `double_option` serde semantics:
    // absent = leave alone, explicit null = clear, value = set.
    // Only include keys the caller explicitly provided.
    const patch: Record<string, unknown> = {};
    if (updates.telemetryEnabled !== undefined) {
      patch.telemetryEnabled = updates.telemetryEnabled;
    }
    if (updates.indexerUrl !== undefined) {
      // string or explicit null — both forwarded; Rust interprets null as clear.
      patch.indexerUrl = updates.indexerUrl;
    }
    const record = await ctx.updateAccountConfig(patch);
    return record as AccountConfig;
  }

  // ---------------------------------------------------------------------------
  // Real-time event streaming (WASM-owned SSE consumer)
  // ---------------------------------------------------------------------------

  /**
   * Start the WASM-level SSE consumer.
   *
   * Spawns a background task inside WASM that connects to the indexer's
   * `/api/events` endpoint, pulls events, and applies them to any
   * installed directory trees via the Rust-side `TreeKeeper`. Once
   * started, `FileManager.watchDirectory` handlers fire automatically
   * as events arrive.
   *
   * Events are parsed and applied entirely in Rust — only serialized
   * snapshots cross into JS. Idempotent: safe to call multiple times
   * (StrictMode double-mount is handled internally).
   *
   * `indexerUrl` is optional. Omitted: resolve via the Opake's priority
   * chain (runtime override → PDS accountConfig → compile-time default).
   * Provided: promoted to the runtime override (priority 1) — it wins
   * over PDS config and persists across subsequent indexer calls on the
   * same Opake instance, so passing it here is equivalent to calling
   * `setIndexerUrl` before start.
   */
  @wrapWasmErrors
  startSseConsumer(indexerUrl?: string): Promise<void> {
    return this.requireContext().startSseConsumer(indexerUrl ?? null);
  }

  /**
   * Stop the WASM SSE consumer. Clears the internal running flag so a
   * subsequent `startSseConsumer` call can spawn a fresh consumer. No
   * crypto material is wiped — call `wipeState()` for that when the
   * session is truly ending (logout, account switch).
   */
  stopSseConsumer(): void {
    const ctx = this.ctx;
    if (ctx) ctx.stopSseConsumer();
  }

  /**
   * Drain every in-memory keeper (directory trees, workspace list,
   * inbox). Drops cached ContentKeys (ZeroizeOnDrop fires) and the
   * decrypted directory-name cache. Call on logout / account switch
   * so one user's crypto state doesn't leak into the next session.
   *
   * Typical teardown order is `stopSseConsumer()` then `wipeState()`
   * — stop the stream first so no events land against freshly-
   * uninstalled scopes. OpakeProvider's unmount does this for you.
   */
  wipeState(): void {
    const ctx = this.ctx;
    if (ctx) ctx.wipeState();
  }

  /**
   * Subscribe to live changes in the workspace list.
   *
   * Fires the handler once immediately with the current snapshot
   * (`loaded: false` with an empty `entries` list if the keeper hasn't
   * been bootstrapped yet), and again on every subsequent mutation —
   * the initial `listWorkspaces` call populates the keeper, and SSE
   * `keyring:upsert` / `keyring:delete` events patch it incrementally.
   *
   * The returned handle is synchronous — registration is kicked off
   * eagerly and `close()` chains onto the pending Promise. This
   * mirrors `FileManager.watchDirectory`, letting React effects use
   * the result without an intermediate Promise.
   *
   * @example
   * ```typescript
   * useEffect(() => {
   *   const watcher = opake.watchWorkspaces((snapshot) => {
   *     setWorkspaces(snapshot.entries);
   *     setLoaded(snapshot.loaded);
   *   });
   *   return () => watcher.close();
   * }, [opake]);
   * ```
   */
  watchWorkspaces(handler: (snapshot: WorkspaceSnapshot) => void): WorkspaceWatcher {
    // WASM calls back with the raw snapshot object — validate + transform
    // via the Zod schema so consumers never see snake_case or untyped values.
    const adapter = (raw: unknown) => {
      let snapshot: WorkspaceSnapshot;
      try {
        snapshot = workspaceSnapshotSchema.parse(raw);
      } catch (err) {
        console.warn("[opake-sdk] watchWorkspaces snapshot parse failed:", err);
        return;
      }
      try {
        handler(snapshot);
      } catch (err) {
        // One broken handler shouldn't break the event loop.
        console.warn("[opake-sdk] watchWorkspaces handler threw:", err);
      }
    };

    const pending = this.requireContext().watchWorkspaces(adapter);
    let closed = false;
    let wasmWatcher: WasmWorkspaceWatcherHandle | null = null;

    pending.then(
      (w) => {
        if (closed) {
          // close() fired before the handle resolved — clean up now.
          void w.close();
          return;
        }
        wasmWatcher = w as WasmWorkspaceWatcherHandle;
      },
      (err: unknown) => {
        console.warn("[opake-sdk] watchWorkspaces registration failed:", err);
      },
    );

    return {
      close: () => {
        if (closed) return;
        closed = true;
        if (wasmWatcher) {
          void wasmWatcher.close();
          wasmWatcher = null;
        }
      },
    };
  }

  // ---------------------------------------------------------------------------
  // Daemon operations
  // ---------------------------------------------------------------------------

  /** Sync a single workspace by keyring URI. Returns null if not a member. */
  @wrapWasmErrors
  @withTokenGuard
  syncWorkspaceByUri(keyringUri: string): Promise<WorkspaceSyncResult | null> {
    return this.requireContext().syncWorkspaceByUri(keyringUri).then(syncSingleResultSchema.parse);
  }

  /** Retry pending shares — resolve recipients and create grants. */
  @wrapWasmErrors
  @withTokenGuard
  retryPendingShares(): Promise<{
    checked: number;
    completed: number;
    expired: number;
    still_pending: number;
    failed: number;
  }> {
    return this.requireContext().retryPendingSharesViaOpake();
  }

  // ---------------------------------------------------------------------------
  // Sharing — inbox + pending shares + cross-PDS grant download
  // ---------------------------------------------------------------------------

  /**
   * Fetch every incoming grant from the Indexer.
   *
   * Also bootstraps the in-memory `InboxKeeper` — any current or future
   * `watchInbox` callers receive a fresh snapshot with `loaded = true`.
   * Once bootstrapped, SSE `grant:upsert` / `grant:delete` events keep
   * the keeper in sync without further `listInbox` round-trips.
   */
  @wrapWasmErrors
  @withTokenGuard
  listInbox(): Promise<readonly InboxGrant[]> {
    return this.requireContext()
      .listInbox()
      .then((raw) => inboxGrantsSchema.parse(raw)) as Promise<readonly InboxGrant[]>;
  }

  /**
   * Download and decrypt a shared document using a grant URI.
   *
   * Cross-PDS — uses the recipient's identity key to unwrap the content
   * key embedded in the grant, then fetches and decrypts the blob from
   * the grant owner's PDS. All network I/O is unauthenticated (public
   * PDS endpoints).
   */
  @wrapWasmErrors
  @withTokenGuard
  downloadFromGrant(grantUri: string): Promise<DownloadResult> {
    return this.requireContext().downloadFromGrant(grantUri).then(downloadResultSchema.parse);
  }

  /**
   * Resolve a grant's document metadata (filename + encrypted fields)
   * without downloading the blob.
   *
   * Cross-PDS — fetches grant + document records from the owner's PDS,
   * unwraps the content key, decrypts metadata. Useful for rendering
   * a "shared with me" list without paying the download cost for
   * every entry.
   */
  @wrapWasmErrors
  @withTokenGuard
  resolveGrantMetadata(grantUri: string): Promise<ResolvedGrantMetadata> {
    return this.requireContext()
      .resolveGrantMetadata(grantUri)
      .then(resolvedGrantMetadataSchema.parse);
  }

  /**
   * List every pending (queued) outgoing share on the caller's PDS.
   *
   * A pending share exists when a recipient hadn't set up Opake yet at
   * the time of sharing. The daemon retries until the recipient
   * publishes a public key or the share expires (7 days).
   */
  @wrapWasmErrors
  @withTokenGuard
  listPendingShares(): Promise<readonly PendingShareEntry[]> {
    return this.requireContext()
      .listPendingShares()
      .then((raw) => pendingShareEntriesSchema.parse(raw)) as Promise<readonly PendingShareEntry[]>;
  }

  /** Cancel (delete) a pending share by its AT-URI. */
  @wrapWasmErrors
  @withTokenGuard
  cancelPendingShare(uri: string): Promise<void> {
    return this.requireContext().cancelPendingShare(uri);
  }

  /**
   * Subscribe to live changes in the inbox (incoming grants).
   *
   * Fires the handler once immediately with the current snapshot
   * (`loaded: false` + empty `entries` if the keeper hasn't been
   * bootstrapped yet), and again on every `grant:upsert` / `grant:delete`
   * SSE event. The initial `listInbox` call populates the keeper.
   *
   * Mirrors `watchWorkspaces`: returns a synchronous handle; registration
   * is kicked off eagerly and `close()` chains onto the pending Promise.
   *
   * @example
   * ```tsx
   * useEffect(() => {
   *   const watcher = opake.watchInbox((snapshot) => {
   *     setInbox(snapshot.entries);
   *     setLoaded(snapshot.loaded);
   *   });
   *   return () => watcher.close();
   * }, [opake]);
   * ```
   */
  watchInbox(handler: (snapshot: InboxSnapshot) => void): InboxWatcher {
    const adapter = (raw: unknown) => {
      let snapshot: InboxSnapshot;
      try {
        snapshot = inboxSnapshotSchema.parse(raw);
      } catch (err) {
        console.warn("[opake-sdk] watchInbox snapshot parse failed:", err);
        return;
      }
      try {
        handler(snapshot);
      } catch (err) {
        console.warn("[opake-sdk] watchInbox handler threw:", err);
      }
    };

    const pending = this.requireContext().watchInbox(adapter);
    let closed = false;
    let wasmWatcher: WasmInboxWatcherHandle | null = null;

    pending.then(
      (w) => {
        if (closed) {
          void w.close();
          return;
        }
        wasmWatcher = w as WasmInboxWatcherHandle;
      },
      (err: unknown) => {
        console.warn("[opake-sdk] watchInbox registration failed:", err);
      },
    );

    return {
      close: () => {
        if (closed) return;
        closed = true;
        if (wasmWatcher) {
          void wasmWatcher.close();
          wasmWatcher = null;
        }
      },
    };
  }

  // ---------------------------------------------------------------------------
  // Device pairing
  // ---------------------------------------------------------------------------
  //
  // The new-device side (create request + await completion) is exposed as
  // static methods — they don't require an Opake instance because the new
  // device has no Identity yet. See the `Opake.createPairRequest` /
  // `Opake.awaitPairCompletion` pair below.
  //
  // The existing-device side stays on the instance: it already has an
  // Identity and the authenticated context to wrap it against the
  // requesting device's ephemeral public key.

  /**
   * Start a device-pairing request from the new device.
   *
   * Must run after `Opake.startLogin` / `Opake.completeLogin` have put an
   * authenticated session in Storage but before `Opake.init` — which would
   * fail with `IdentityMissing` at this stage. The returned fingerprint
   * (first bytes of `ephemeralPublicKey`) is for out-of-band comparison
   * with the approving device.
   */
  static async createPairRequest(
    storage: import("./storage").Storage,
    did: string,
  ): Promise<import("./types").PairRequestResult> {
    return pairingCreate(storage, did);
  }

  /**
   * Poll until the paired device approves. Resolves once the received
   * identity has been persisted to Storage; `Opake.init` then succeeds.
   */
  static async awaitPairCompletion(
    storage: import("./storage").Storage,
    did: string,
    requestRkey: string,
    options?: AwaitPairOptions,
  ): Promise<void> {
    return pairingAwait(storage, did, requestRkey, options);
  }

  /** Cancel an in-flight pair request. Wipes pair state on both sides. */
  static async cancelPairRequest(
    storage: import("./storage").Storage,
    did: string,
    requestRkey: string,
  ): Promise<void> {
    return pairingCancel(storage, did, requestRkey);
  }

  /** List pending pair requests on this account. */
  @wrapWasmErrors
  @withTokenGuard
  listPairRequests(): Promise<readonly import("./types").PendingPairRequest[]> {
    return pairingList(this.requireContext());
  }

  /** Approve a pair request (existing device). Encrypts and sends the identity. */
  @wrapWasmErrors
  @withTokenGuard
  approvePairRequest(requestUri: string, ephemeralPublicKey: Uint8Array): Promise<void> {
    return pairingApprove(this.requireContext(), requestUri, ephemeralPublicKey);
  }

  /** Delete expired pair requests and orphaned responses. */
  @wrapWasmErrors
  @withTokenGuard
  cleanupExpiredPairRequests(): Promise<number> {
    return pairingCleanupExpired(this.requireContext());
  }

  /** Delete stale grants whose recipients have no valid public key. */
  @wrapWasmErrors
  @withTokenGuard
  healStaleGrants(): Promise<number> {
    return this.requireContext().healStaleGrants();
  }

  // ---------------------------------------------------------------------------
  // Invitations
  // ---------------------------------------------------------------------------

  /** Create a workspace invitation. Returns `{ uri, token }`. */
  @wrapWasmErrors
  @withTokenGuard
  createInvitation(keyringUri: string, role: string): Promise<{ uri: string; token: string }> {
    return this.requireContext().createInvitation(keyringUri, role) as Promise<{
      uri: string;
      token: string;
    }>;
  }

  /** List all invitations on this account. */
  @wrapWasmErrors
  @withTokenGuard
  async listInvitations(): Promise<readonly import("./types").InvitationEntry[]> {
    const raw = (await this.requireContext().listInvitations()) as readonly Record<
      string,
      unknown
    >[];
    return raw.map((r) => ({
      uri: r.uri as string,
      target: r.target as string,
      invitationType: (r.invitation_type ?? r.invitationType) as string,
      role: (r.role ?? null) as string | null,
      token: r.token as string,
      maxUses: (r.max_uses ?? r.maxUses ?? null) as number | null,
      uses: (r.uses ?? 0) as number,
      expiresAt: (r.expires_at ?? r.expiresAt ?? null) as string | null,
      createdAt: (r.created_at ?? r.createdAt) as string,
    }));
  }

  /** Revoke (delete) an invitation. */
  @wrapWasmErrors
  @withTokenGuard
  revokeInvitation(invitationUri: string): Promise<void> {
    return this.requireContext().revokeInvitation(invitationUri);
  }

  // ---------------------------------------------------------------------------
  // Daemon task definitions
  // ---------------------------------------------------------------------------

  /**
   * Load background task definitions from the core registry.
   *
   * Returns the canonical task list with camelCase field names, excluding
   * `session-refresh` (handled by the SDK's `@withTokenGuard`).
   */
  static async taskDefs(): Promise<readonly import("./types").TaskDef[]> {
    const wasm = await initWasm();
    const raw = wasm.daemonTaskDefs() as readonly {
      name: string;
      interval_seconds: number;
      description: string;
    }[];
    return raw
      .filter((t) => t.name !== "session-refresh")
      .map((t) => ({
        name: t.name,
        intervalSeconds: t.interval_seconds,
        description: t.description,
      }));
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
      unregisterCleanup(this);
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
