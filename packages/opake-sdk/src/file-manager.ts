// FileManager — file operations within a cabinet or workspace context.
//
// Wraps the WASM WasmFileManagerHandle. Created via `opake.cabinet()` or
// `opake.workspace(keyringUri)`. Call `.dispose()` when done to release the
// context back to the parent Opake instance.

import type {
  MutationResult,
  UploadResult,
  DirectoryTreeSnapshot,
  DocumentMetadata,
  DownloadResult,
  DeleteRecursiveResult,
  GrantEntry,
  ShareWriteResult,
} from "./types";
import type { DocumentMetadataResolution } from "./schemas";
import { parseWasmError, wrapWasmErrors } from "./errors";
import { registerCleanup, unregisterCleanup } from "./finalizer";
import {
  downloadResultSchema,
  deleteRecursiveResultSchema,
  directoryTreeSnapshotSchema,
  documentMetadataSchema,
  documentMetadataResolutionsSchema,
  grantEntriesSchema,
  treeWithMetadataSchema,
} from "./schemas";

// WASM handle type (the actual WasmFileManagerHandle from opake-wasm)
type WasmFileManager = {
  upload(
    plaintext: Uint8Array,
    filename: string,
    mimeType: string,
    description: string | null,
    tags: string[] | null,
    directoryUri: string | null,
  ): Promise<unknown>;
  download(documentUri: string): Promise<unknown>;
  delete(documentUri: string, parentDirectoryUri: string): Promise<unknown>;
  moveEntry(entryUri: string, sourceDir: string, targetDir: string): Promise<unknown>;
  createDirectory(name: string, parentUri: string | null): Promise<unknown>;
  ensureRoot(): Promise<string>;
  loadTree(): Promise<unknown>;
  loadTreeWithMetadata(metadataForDir: string | null): Promise<unknown>;
  syncAndLoadTree(metadataForDir: string | null): Promise<unknown>;
  getDocumentMetadata(documentUri: string): Promise<unknown>;
  resolveDocumentMetadataFor(documentUris: string[]): Promise<unknown>;
  renameDirectory(directoryUri: string, newName: string): Promise<unknown>;
  updateMetadata(
    documentUri: string,
    name: string | null,
    tags: string[] | null,
    description: string | null,
  ): Promise<unknown>;
  updateContent(documentUri: string, newPlaintext: Uint8Array): Promise<unknown>;
  deleteRecursive(uri: string): Promise<unknown>;
  share(
    documentUri: string,
    recipient: string,
    confirmedUnverifiedKeys: Uint8Array | null,
    permissions: string,
    note: string | null,
  ): Promise<unknown>;
  shareApprovalChallenge(documentUri: string, recipient: string): Promise<unknown>;
  preparePendingShareRecipient(
    documentUri: string,
    recipient: string,
  ): Promise<WasmPendingShareRecipient>;
  revokeShare(grantUri: string): Promise<void>;
  listShares(): Promise<unknown>;
  createPendingShare(
    documentUri: string,
    recipient: WasmPendingShareRecipient,
    allowUnverifiedFirstPublication: boolean,
    permissions: string,
    note: string | null,
  ): Promise<string>;
  watchDirectory(
    directoryUri: string,
    callback: (snapshot: unknown) => void,
  ): Promise<WasmDirectoryWatcher>;
  free(): void;
};

/** Opaque WASM-held DID resolution for a queued first-publication handoff. */
type WasmPendingShareRecipient = { readonly did: string; free(): void };

/**
 * A recipient resolved inside WASM for a pending-share confirmation.
 *
 * The DID is read-only display data for the queue-time consent warning. Retain
 * this value only until the user accepts or declines that warning, then pass
 * it to `createPendingShare`.
 */
export class PendingShareRecipient {
  private handle: WasmPendingShareRecipient | null;

  /** @internal — obtain one from `preparePendingShareRecipient`. */
  constructor(handle: WasmPendingShareRecipient) {
    this.handle = handle;
  }

  /** The exact DID that this queued consent will authorize. */
  get did(): string {
    if (!this.handle) throw new Error("PendingShareRecipient has already been consumed");
    return this.handle.did;
  }

  /** @internal */
  take(): WasmPendingShareRecipient {
    if (!this.handle) throw new Error("PendingShareRecipient has already been consumed");
    const handle = this.handle;
    this.handle = null;
    return handle;
  }

  /** Discard an unaccepted queue-time resolution. */
  dispose(): void {
    this.handle?.free();
    this.handle = null;
  }
}

/** WASM DirectoryWatcher handle — returned by watchDirectory. */
type WasmDirectoryWatcher = {
  close(): Promise<void>;
  free(): void;
};

/**
 * Handle returned by `FileManager.watchDirectory`. Call `.close()` to
 * unsubscribe — typically from a React useEffect cleanup.
 */
export interface DirectoryWatcher {
  /** Stop receiving notifications. Idempotent. */
  close(): void;
}

/**
 * File operations within a cabinet or workspace.
 *
 * Created via `opake.cabinet()` or `opake.workspace()`. Operates
 * on directories and documents within the bound context.
 *
 * Token management: the `Opake` class proactively refreshes tokens
 * before expiry via `@withTokenGuard`. If a token expires mid-operation
 * (e.g., during a slow upload), the WASM XRPC client handles it
 * reactively by retrying with a fresh token.
 *
 * Call `.dispose()` when done to release the context. Supports the
 * TC39 Explicit Resource Management proposal (`using`).
 *
 * @example
 * ```typescript
 * const cabinet = opake.cabinet();
 * try {
 *   await cabinet.upload(data, "photo.jpg", "image/jpeg");
 *   const tree = await cabinet.loadTree();
 * } finally {
 *   cabinet.dispose();
 * }
 *
 * // Or with `using` (when available):
 * using cabinet = opake.cabinet();
 * await cabinet.upload(data, "photo.jpg", "image/jpeg");
 * ```
 */
export class FileManager {
  private handle: WasmFileManager | null;

  // In-flight operation tracking. Every async method that touches the WASM
  // handle runs through `track()`, which holds a borrow of the JS handle for
  // the lifetime of its future (wasm-bindgen borrows `&self` across the whole
  // async fn). Calling `handle.free()` while such a borrow is live panics with
  // "attempted to take ownership of Rust value while it was borrowed". So a
  // `dispose()` that arrives mid-operation is deferred: no new ops are
  // admitted, and the actual free fires when the last in-flight op settles.
  private inFlightOps = 0;
  private disposeRequested = false;
  private disposed = false;

  /** @internal — use `opake.cabinet()` or `opake.workspace(keyringUri)` instead. */
  constructor(handle: WasmFileManager) {
    this.handle = handle;
    registerCleanup(this, handle, this);
  }

  /**
   * Run a handle-touching operation under in-flight tracking.
   *
   * Rejects immediately if the manager is disposed (or a deferred dispose is
   * pending — no new ops once teardown starts). Otherwise increments the
   * in-flight count for the duration of the op's future and frees the handle
   * once the count returns to zero if a dispose was requested meanwhile.
   */
  private track<T>(op: () => Promise<T>): Promise<T> {
    if (!this.handle || this.disposed) {
      return Promise.reject(parseWasmError(new Error("FileManager has been disposed")));
    }
    this.inFlightOps += 1;
    // The op must run synchronously so the WASM `&self` borrow is taken before
    // any `dispose()` on the same tick can observe the in-flight count. The
    // async wrapper invokes `op()` synchronously while turning a synchronous
    // throw into a rejection, so `finally` always runs and the in-flight count
    // can never be stranded above zero.
    return (async () => op())().finally(() => this.finishOp());
  }

  private finishOp(): void {
    this.inFlightOps -= 1;
    if (this.inFlightOps === 0 && this.disposeRequested) {
      this.performFree();
    }
  }

  private performFree(): void {
    if (this.handle) {
      unregisterCleanup(this);
      this.handle.free();
      this.handle = null;
    }
  }

  // ---------------------------------------------------------------------------
  // File operations
  // ---------------------------------------------------------------------------

  /**
   * Upload a file.
   *
   * Encrypts the file client-side, uploads the ciphertext blob to the PDS,
   * and creates the document record with encrypted metadata.
   *
   * @param data - Raw file contents.
   * @param filename - Display name for the file.
   * @param mimeType - MIME type (e.g., "image/jpeg", "application/pdf").
   * @param options - Optional metadata: description, tags, target directory.
   * @returns Upload result with the document URI.
   *
   * @throws {OpakeError} kind "Auth" if the session is expired.
   * @throws {OpakeError} kind "Encryption" if encryption fails.
   *
   * @example
   * ```typescript
   * const file = new Uint8Array(await response.arrayBuffer());
   * const result = await cabinet.upload(file, "report.pdf", "application/pdf");
   * console.log("Uploaded:", result.uri);
   *
   * // With tags and description:
   * await cabinet.upload(file, "report.pdf", "application/pdf", {
   *   description: "Q3 financials",
   *   tags: ["finance", "quarterly"],
   *   directoryUri: parentDir,
   * });
   * ```
   */
  @wrapWasmErrors
  upload(
    data: Uint8Array,
    filename: string,
    mimeType: string,
    options?: {
      description?: string;
      tags?: readonly string[];
      directoryUri?: string;
    },
  ): Promise<UploadResult> {
    return this.track(() =>
      this.requireHandle().upload(
        data,
        filename,
        mimeType,
        options?.description ?? null,
        options?.tags ? [...options.tags] : null,
        options?.directoryUri ?? null,
      ),
    ) as Promise<UploadResult>;
  }

  /**
   * Download and decrypt a file.
   *
   * Fetches the encrypted blob from the PDS, unwraps the content key,
   * and returns the decrypted plaintext with the original filename.
   *
   * @param documentUri - AT URI of the document record.
   * @returns Decrypted file contents and original filename.
   *
   * @throws {OpakeError} kind "NotFound" if the document doesn't exist.
   * @throws {OpakeError} kind "Decryption" if decryption fails.
   *
   * @example
   * ```typescript
   * const { filename, data } = await cabinet.download(documentUri);
   * const blob = new Blob([data], { type: "application/octet-stream" });
   * ```
   */
  @wrapWasmErrors
  download(documentUri: string): Promise<DownloadResult> {
    return this.track(() =>
      this.requireHandle().download(documentUri).then(downloadResultSchema.parse),
    );
  }

  /**
   * Delete a document.
   *
   * Removes the document record and its blob from the PDS and atomically
   * removes the entry from its parent directory.
   *
   * @param documentUri - AT URI of the document to delete.
   * @param parentDirectoryUri - Directory containing the document. Required:
   *   the delete is atomic with removing this entry from the parent. Callers
   *   must resolve the parent before calling; passing the wrong one is caught
   *   by `prepare_remove_entry` (NotFound error), passing a valid-but-stale
   *   one is not.
   *
   * @throws {OpakeError} kind "NotFound" if the document doesn't exist in the
   *   parent directory.
   */
  @wrapWasmErrors
  delete(documentUri: string, parentDirectoryUri: string): Promise<MutationResult> {
    return this.track(() =>
      this.requireHandle().delete(documentUri, parentDirectoryUri),
    ) as Promise<MutationResult>;
  }

  /**
   * Move an entry (document or directory) between directories.
   *
   * Atomic for owners (remove from source + add to target in one call).
   * Proposed for workspace members.
   *
   * @param entryUri - URI of the entry to move.
   * @param sourceDirUri - Current parent directory URI.
   * @param targetDirUri - Destination directory URI.
   */
  @wrapWasmErrors
  move(entryUri: string, sourceDirUri: string, targetDirUri: string): Promise<MutationResult> {
    return this.track(() =>
      this.requireHandle().moveEntry(entryUri, sourceDirUri, targetDirUri),
    ) as Promise<MutationResult>;
  }

  // ---------------------------------------------------------------------------
  // Directory operations
  // ---------------------------------------------------------------------------

  /**
   * Create a new directory.
   *
   * @param name - Display name for the directory.
   * @param parentUri - Parent directory URI. Omit to create under the root.
   * @returns The URI of the created directory.
   *
   * @example
   * ```typescript
   * const result = await cabinet.createDirectory("photos");
   * console.log("Created directory:", result.uri);
   * ```
   */
  @wrapWasmErrors
  createDirectory(name: string, parentUri?: string): Promise<UploadResult> {
    return this.track(() =>
      this.requireHandle().createDirectory(name, parentUri ?? null),
    ) as Promise<UploadResult>;
  }

  /**
   * Create a directory if it doesn't already exist.
   *
   * Loads the tree, checks if a directory with the given name exists in
   * the parent, and returns it if so. Otherwise creates a new one.
   *
   * @param name - Directory name.
   * @param parentUri - Parent directory URI.
   * @returns The existing or newly created directory URI + whether it was created.
   */
  async createDirectoryIfNotExists(
    name: string,
    parentUri?: string,
  ): Promise<{ uri: string; created: boolean }> {
    const tree = await this.loadTree();
    const parent = parentUri ? tree.directories[parentUri] : null;
    if (parent) {
      for (const entry of parent.entries) {
        if (entry.type === "directory") {
          const dir = tree.directories[entry.uri];
          if (dir?.name === name) {
            return { uri: entry.uri, created: false };
          }
        }
      }
    }
    const result = await this.createDirectory(name, parentUri);
    return { uri: result.uri, created: true };
  }

  /**
   * Find a document by filename within a directory.
   *
   * Loads the tree and metadata, searches for a document with the given
   * name. Returns the URI and metadata, or null if not found.
   *
   * @param directoryUri - Directory to search in.
   * @param filename - Document name to match.
   */
  async findDocument(
    directoryUri: string,
    filename: string,
  ): Promise<{ uri: string; metadata: DocumentMetadata } | null> {
    const { metadata } = await this.loadTreeWithMetadata(directoryUri);
    for (const [uri, meta] of Object.entries(metadata)) {
      if (meta.name === filename) {
        return { uri, metadata: meta };
      }
    }
    return null;
  }

  /**
   * Ensure the root directory exists, creating it if needed.
   *
   * @returns The URI of the root directory.
   */
  @wrapWasmErrors
  ensureRoot(): Promise<string> {
    return this.track(() => this.requireHandle().ensureRoot());
  }

  /**
   * Load the full directory tree with decrypted names.
   *
   * Returns a snapshot of all directories and their entries. Use this
   * to build a file browser UI.
   *
   * @returns The directory tree snapshot.
   *
   * @example
   * ```typescript
   * const tree = await cabinet.loadTree();
   * if (tree.rootUri) {
   *   const root = tree.directories[tree.rootUri];
   *   console.log("Root entries:", root.entries.length);
   * }
   * ```
   */
  @wrapWasmErrors
  loadTree(): Promise<DirectoryTreeSnapshot> {
    return this.track(() =>
      (this.requireHandle().loadTree() as Promise<{ snapshot: unknown }>)
        // Zod 4 z.record() with .transform() inner schemas loses type info.
        // Runtime validation is correct — the cast bridges the inference gap.
        .then((r) => directoryTreeSnapshotSchema.parse(r.snapshot) as DirectoryTreeSnapshot),
    );
  }

  /**
   * Load tree, apply proposals, and resolve metadata — the full sync cycle.
   *
   * Unlike `loadTree()`, this applies pending member proposals (PDS writes
   * for workspace owners) and resolves document metadata.
   *
   * @param directoryUri - Directory to resolve metadata for. `"*"` for all, omit for none.
   */
  @wrapWasmErrors
  syncAndLoadTree(
    directoryUri?: string,
  ): Promise<{ snapshot: DirectoryTreeSnapshot; metadata: Record<string, DocumentMetadata> }> {
    return this.track(() =>
      this.requireHandle()
        .syncAndLoadTree(directoryUri ?? null)
        .then(treeWithMetadataSchema.parse),
    );
  }

  /**
   * Load tree + metadata (read-only, no proposal application).
   *
   * Like `loadTree()` but also decrypts document metadata for the specified
   * directory. Does NOT apply proposals or write to the PDS.
   *
   * @param directoryUri - Directory to resolve metadata for. `"*"` for all, omit for root.
   */
  @wrapWasmErrors
  loadTreeWithMetadata(
    directoryUri?: string,
  ): Promise<{ snapshot: DirectoryTreeSnapshot; metadata: Record<string, DocumentMetadata> }> {
    return this.track(() =>
      this.requireHandle()
        .loadTreeWithMetadata(directoryUri ?? null)
        .then(treeWithMetadataSchema.parse),
    );
  }

  /**
   * Fetch and decrypt metadata for a single document.
   *
   * Includes timestamps from the PDS record (`createdAt`, `modifiedAt`).
   *
   * @param documentUri - AT URI of the document.
   *
   * @example
   * ```typescript
   * const meta = await cabinet.getDocumentMetadata(docUri);
   * console.log(meta.name, meta.size, meta.createdAt);
   * ```
   */
  @wrapWasmErrors
  getDocumentMetadata(documentUri: string): Promise<DocumentMetadata> {
    return this.track(() =>
      this.requireHandle().getDocumentMetadata(documentUri).then(documentMetadataSchema.parse),
    );
  }

  /**
   * Resolve name-hydration status for an explicit list of document URIs.
   *
   * Unlike {@link loadTreeWithMetadata}, this resolves the exact URIs passed
   * rather than re-deriving a tree and iterating its entries — so a document
   * a live projection (the SSE tree keeper) already lists still resolves even
   * when a `loadTree`-derived tree hasn't caught up to it yet. Each URI comes
   * back tagged: `resolved` with metadata, `retryable` (not visible yet —
   * poll again), or `undecryptable` (definitive; this caller can't decrypt
   * it). URIs the resolver couldn't classify are simply absent from the map.
   *
   * @param documentUris - Document AT URIs to resolve.
   */
  @wrapWasmErrors
  resolveDocumentMetadataFor(
    documentUris: readonly string[],
  ): Promise<Readonly<Record<string, DocumentMetadataResolution>>> {
    return this.track(() =>
      this.requireHandle()
        .resolveDocumentMetadataFor([...documentUris])
        .then((raw) => documentMetadataResolutionsSchema.parse(raw)),
    );
  }

  /**
   * Rename a directory.
   *
   * Re-encrypts the directory metadata with the new name.
   *
   * @param directoryUri - URI of the directory to rename.
   * @param newName - New display name.
   */
  @wrapWasmErrors
  renameDirectory(directoryUri: string, newName: string): Promise<MutationResult> {
    return this.track(() =>
      this.requireHandle().renameDirectory(directoryUri, newName),
    ) as Promise<MutationResult>;
  }

  /**
   * Recursively delete a directory and all its contents.
   *
   * Deletes all documents and subdirectories depth-first.
   *
   * @param directoryUri - URI of the directory to delete.
   * @returns Counts of deleted documents and directories.
   */
  @wrapWasmErrors
  deleteRecursive(directoryUri: string): Promise<DeleteRecursiveResult> {
    return this.track(() =>
      this.requireHandle().deleteRecursive(directoryUri).then(deleteRecursiveResultSchema.parse),
    );
  }

  // ---------------------------------------------------------------------------
  // Metadata
  // ---------------------------------------------------------------------------

  /**
   * Update a document's encrypted metadata.
   *
   * @param documentUri - URI of the document.
   * @param updates - Fields to update (null fields are left unchanged).
   */
  @wrapWasmErrors
  updateMetadata(
    documentUri: string,
    updates: { filename?: string; description?: string; tags?: readonly string[] },
  ): Promise<MutationResult> {
    return this.track(() =>
      this.requireHandle().updateMetadata(
        documentUri,
        updates.filename ?? null,
        updates.tags ? [...updates.tags] : null,
        updates.description ?? null,
      ),
    ) as Promise<MutationResult>;
  }

  /**
   * Replace a document's encrypted content.
   *
   * @param documentUri - URI of the document.
   * @param newContent - New file contents (will be encrypted).
   */
  @wrapWasmErrors
  updateContent(documentUri: string, newContent: Uint8Array): Promise<MutationResult> {
    return this.track(() =>
      this.requireHandle().updateContent(documentUri, newContent),
    ) as Promise<MutationResult>;
  }

  // ---------------------------------------------------------------------------
  // Sharing (cabinet only)
  // ---------------------------------------------------------------------------

  /**
   * Share a document with another user.
   *
   * Creates a grant record that wraps the document's content key to the
   * recipient's public encryption key.
   *
   * @param documentUri - URI of the document to share.
   * @param recipient - Handle or DID of the recipient.
   * @param confirmedUnverifiedKeys - Exact 32-byte confirmation from the
   * inspection call when the recipient is unverified.
   * @param role - Access role ("read" or "write").
   */
  @wrapWasmErrors
  share(
    documentUri: string,
    recipient: string,
    confirmedUnverifiedKeys: Uint8Array | null,
    permissions: string,
    note?: string,
  ): Promise<ShareWriteResult> {
    return this.track(() =>
      this.requireHandle().share(
        documentUri,
        recipient,
        confirmedUnverifiedKeys,
        permissions,
        note ?? null,
      ),
    ) as Promise<ShareWriteResult>;
  }

  /**
   * Inspect the exact confirmation required to share with an unverified
   * recipient. Pass `confirmation` unchanged to {@link share}; `null` means
   * their current key record is verified.
   */
  @wrapWasmErrors
  shareApprovalChallenge(
    documentUri: string,
    recipient: string,
  ): Promise<{ did: string; confirmation: Uint8Array | null }> {
    return this.track(() =>
      this.requireHandle().shareApprovalChallenge(documentUri, recipient),
    ) as Promise<{ did: string; confirmation: Uint8Array | null }>;
  }

  /**
   * Revoke a previously created share grant.
   *
   * @param grantUri - URI of the grant record to delete.
   */
  @wrapWasmErrors
  revokeShare(grantUri: string): Promise<void> {
    return this.track(() => this.requireHandle().revokeShare(grantUri));
  }

  /**
   * List every grant on the caller's PDS (cabinet only).
   *
   * Returns all outgoing shares regardless of which document they target —
   * callers filter by `grant.document` for per-document views. Metadata
   * stays encrypted on the wire; callers that need filenames do a
   * separate `getDocumentMetadata` lookup.
   */
  @wrapWasmErrors
  listShares(): Promise<readonly GrantEntry[]> {
    return this.track(() =>
      this.requireHandle()
        .listShares()
        .then((raw) => grantEntriesSchema.parse(raw)),
    );
  }

  /**
   * Queue a pending share for a recipient who hasn't set up Opake yet.
   *
   * Writes an `at.opake.pendingShare` record to the caller's PDS. The
   * daemon retries periodically — when the recipient publishes their
   * public key, the pending share is replaced with a real grant and the
   * record is deleted. Pending shares expire after 7 days.
   *
   * @param documentUri - URI of the document to share.
   * @param recipient - An opaque WASM-held recipient resolution returned by
   * `preparePendingShareRecipient` for this explicit consent interaction.
   * @param allowUnverifiedFirstPublication - Explicit first-publication
   * permission bound to the DID resolved from `recipient`.
   * @param permissions - Access role (typically `"read"`).
   * @param note - Optional message carried through to the resulting grant.
   * @returns The URI of the created pending share record.
   */
  @wrapWasmErrors
  createPendingShare(
    documentUri: string,
    recipient: PendingShareRecipient,
    allowUnverifiedFirstPublication: boolean,
    permissions: string,
    note: string | null,
  ): Promise<string> {
    // The WASM method takes the recipient by value: wasm-bindgen moves it out
    // of the JS wrapper, so the handle is consumed (and freed) by the call
    // itself. Freeing it again here would touch a null pointer.
    const wasmRecipient = recipient.take();
    return this.track(() =>
      this.requireHandle().createPendingShare(
        documentUri,
        wasmRecipient,
        allowUnverifiedFirstPublication,
        permissions,
        note,
      ),
    );
  }

  /** Resolve a pending-share recipient inside WASM for explicit queue consent. */
  @wrapWasmErrors
  preparePendingShareRecipient(
    documentUri: string,
    recipient: string,
  ): Promise<PendingShareRecipient> {
    return this.track(() =>
      this.requireHandle().preparePendingShareRecipient(documentUri, recipient).then((handle) =>
        new PendingShareRecipient(handle),
      ),
    );
  }

  // ---------------------------------------------------------------------------
  // Live tree subscriptions (SSE-driven)
  // ---------------------------------------------------------------------------

  /**
   * Subscribe to live changes for a specific directory.
   *
   * Fires the handler with the current snapshot once on registration
   * (if the tree has been loaded), then again whenever an SSE event
   * affects the tree. The handler receives `null` when the watched
   * directory is deleted — the watcher auto-closes after that call.
   *
   * Must be paired with `opake.startSseConsumer(indexerUrl)` to actually
   * receive events. Without the consumer, the watcher only fires once
   * with the initial snapshot.
   *
   * @param directoryUri - The AT URI of the directory to watch.
   * @param handler - Callback fired with a fresh snapshot per change.
   * @returns A watcher handle. Call `.close()` on unmount to unsubscribe.
   *
   * @example
   * ```typescript
   * useEffect(() => {
   *   const watcher = fm.watchDirectory(dirUri, (snapshot) => {
   *     if (snapshot === null) {
   *       // directory was deleted — route away
   *       navigate("/");
   *       return;
   *     }
   *     setTree(snapshot);
   *   });
   *   return () => watcher.close();
   * }, [dirUri]);
   * ```
   */
  watchDirectory(
    directoryUri: string,
    handler: (snapshot: DirectoryTreeSnapshot | null) => void,
  ): DirectoryWatcher {
    // The WASM binding is async (awaits the tree keeper mutex), but we
    // want to return a synchronous handle so React effects can use it
    // directly without an intermediate Promise. Kick off the registration
    // eagerly and expose a close() that chains onto the promise.
    const adapter = (snapshot: unknown) => {
      // WASM calls back with either the serialized snapshot or null.
      if (snapshot === null) {
        handler(null);
        return;
      }
      try {
        // The WASM-side snapshot is pre-serialized; trust the shape but
        // skip the full Zod parse for hot-path perf (React will re-render
        // regardless).
        handler(snapshot as DirectoryTreeSnapshot);
      } catch (err) {
        // One broken handler shouldn't break the event loop.
        console.warn("[opake-sdk] watchDirectory handler threw:", err);
      }
    };

    // Registration borrows the handle for the duration of its future, so it
    // counts as an in-flight op — a dispose() racing the registration is
    // deferred until it settles (see `track`).
    const handle = this.requireHandle();
    this.inFlightOps += 1;
    const pending = handle.watchDirectory(directoryUri, adapter);
    let closed = false;
    let wasmWatcher: WasmDirectoryWatcher | null = null;

    pending.then(
      (w) => {
        if (closed) {
          // close() was called before the handle resolved — clean up now.
          void w.close();
          return;
        }
        wasmWatcher = w;
      },
      (err: unknown) => {
        console.warn("[opake-sdk] watchDirectory registration failed:", err);
      },
    );
    void pending.finally(() => this.finishOp());

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
  // Lifecycle
  // ---------------------------------------------------------------------------

  /**
   * Release the FileManager's context.
   *
   * After calling `dispose()`, all methods will throw. The parent Opake
   * instance can create new FileManagers after this.
   */
  dispose(): void {
    // Idempotent: a second dispose (StrictMode double-unmount, disposeAll
    // after an individual release) is a no-op once teardown has begun.
    if (this.disposed) return;
    this.disposed = true;

    if (this.inFlightOps > 0) {
      // Ops are still borrowing the handle — defer the free until they
      // drain. Detach the finalizer now so a GC sweep can't race the free.
      this.disposeRequested = true;
      unregisterCleanup(this);
      return;
    }

    this.performFree();
  }

  /** TC39 Explicit Resource Management support. */
  [Symbol.dispose](): void {
    this.dispose();
  }

  private requireHandle(): WasmFileManager {
    if (!this.handle || this.disposed) {
      throw parseWasmError(new Error("FileManager has been disposed"));
    }
    return this.handle;
  }
}
