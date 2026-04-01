// FileManager — file operations within a cabinet or workspace context.
//
// Wraps the WASM WasmFileManagerHandle. Created via `opake.cabinet()` or
// `opake.workspaceFromKey()`. Call `.dispose()` when done to release the
// context back to the parent Opake instance.

import type {
  DeleteRecursiveResult,
  DirectoryTreeSnapshot,
  DocumentMetadata,
  DownloadResult,
  MutationResult,
  UploadResult,
} from "./types";
import { parseWasmError } from "./errors";

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
  delete(documentUri: string, parentDirectoryUri: string | null): Promise<unknown>;
  moveEntry(entryUri: string, sourceDir: string, targetDir: string): Promise<unknown>;
  createDirectory(name: string, parentUri: string | null): Promise<unknown>;
  ensureRoot(): Promise<string>;
  loadTree(): Promise<unknown>;
  loadTreeWithMetadata(metadataForDir: string | null): Promise<unknown>;
  syncAndLoadTree(metadataForDir: string | null): Promise<unknown>;
  getDocumentMetadata(documentUri: string): Promise<unknown>;
  renameDirectory(directoryUri: string, newName: string): Promise<unknown>;
  updateMetadata(
    documentUri: string,
    name: string | null,
    tags: string[] | null,
    description: string | null,
  ): Promise<unknown>;
  updateContent(documentUri: string, newPlaintext: Uint8Array): Promise<unknown>;
  deleteRecursive(uri: string): Promise<unknown>;
  share(documentUri: string, recipientDid: string, recipientPublicKey: Uint8Array, permissions: string, note: string | null): Promise<unknown>;
  revokeShare(grantUri: string): Promise<void>;
  listShares(): Promise<unknown>;
  syncAndApplyProposals(): Promise<number>;
  isOwner(): boolean;
  free(): void;
};

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

  /** @internal — use `opake.cabinet()` or `opake.workspaceFromKey()` instead. */
  constructor(handle: WasmFileManager) {
    this.handle = handle;
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
  async upload(
    data: Uint8Array,
    filename: string,
    mimeType: string,
    options?: {
      description?: string;
      tags?: readonly string[];
      directoryUri?: string;
    },
  ): Promise<UploadResult> {
    const h = this.requireHandle();
    try {
      const result = await h.upload(
        data,
        filename,
        mimeType,
        options?.description ?? null,
        options?.tags ? [...options.tags] : null,
        options?.directoryUri ?? null,
      );
      return result as UploadResult;
    } catch (e) {
      throw parseWasmError(e);
    }
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
  async download(documentUri: string): Promise<DownloadResult> {
    const h = this.requireHandle();
    try {
      const result = await h.download(documentUri);
      const parsed = result as { filename: string; plaintext: Uint8Array };
      return { filename: parsed.filename, data: parsed.plaintext };
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * Delete a document.
   *
   * Removes the document record and its blob from the PDS. If a parent
   * directory URI is provided, also removes the entry from that directory.
   *
   * @param documentUri - AT URI of the document to delete.
   * @param parentDirectoryUri - Directory containing the document (for entry cleanup).
   *
   * @throws {OpakeError} kind "NotFound" if the document doesn't exist.
   */
  async delete(documentUri: string, parentDirectoryUri?: string): Promise<MutationResult> {
    const h = this.requireHandle();
    try {
      const result = await h.delete(documentUri, parentDirectoryUri ?? null);
      return result as MutationResult;
    } catch (e) {
      throw parseWasmError(e);
    }
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
  async move(entryUri: string, sourceDirUri: string, targetDirUri: string): Promise<MutationResult> {
    const h = this.requireHandle();
    try {
      const result = await h.moveEntry(entryUri, sourceDirUri, targetDirUri);
      return result as MutationResult;
    } catch (e) {
      throw parseWasmError(e);
    }
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
  async createDirectory(name: string, parentUri?: string): Promise<UploadResult> {
    const h = this.requireHandle();
    try {
      const result = await h.createDirectory(name, parentUri ?? null);
      return result as UploadResult;
    } catch (e) {
      throw parseWasmError(e);
    }
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
          if (dir && dir.name === name) {
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
  async ensureRoot(): Promise<string> {
    const h = this.requireHandle();
    try {
      return await h.ensureRoot();
    } catch (e) {
      throw parseWasmError(e);
    }
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
  async loadTree(): Promise<DirectoryTreeSnapshot> {
    const h = this.requireHandle();
    try {
      const result = await h.loadTree();
      const parsed = result as { snapshot: DirectoryTreeSnapshot };
      return parsed.snapshot;
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * Load tree, apply proposals, and resolve metadata — the full sync cycle.
   *
   * Unlike `loadTree()`, this applies pending member proposals (PDS writes
   * for workspace owners) and resolves document metadata.
   *
   * @param directoryUri - Directory to resolve metadata for. `"*"` for all, omit for none.
   */
  async syncAndLoadTree(
    directoryUri?: string,
  ): Promise<{ snapshot: DirectoryTreeSnapshot; metadata: Record<string, DocumentMetadata> }> {
    const h = this.requireHandle();
    try {
      const result = await h.syncAndLoadTree(directoryUri ?? null);
      return result as { snapshot: DirectoryTreeSnapshot; metadata: Record<string, DocumentMetadata> };
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * Load tree + metadata (read-only, no proposal application).
   *
   * Like `loadTree()` but also decrypts document metadata for the specified
   * directory. Does NOT apply proposals or write to the PDS.
   *
   * @param directoryUri - Directory to resolve metadata for. `"*"` for all, omit for root.
   */
  async loadTreeWithMetadata(
    directoryUri?: string,
  ): Promise<{ snapshot: DirectoryTreeSnapshot; metadata: Record<string, DocumentMetadata> }> {
    const h = this.requireHandle();
    try {
      const result = await h.loadTreeWithMetadata(directoryUri ?? null);
      return result as { snapshot: DirectoryTreeSnapshot; metadata: Record<string, DocumentMetadata> };
    } catch (e) {
      throw parseWasmError(e);
    }
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
  async getDocumentMetadata(documentUri: string): Promise<DocumentMetadata> {
    const h = this.requireHandle();
    try {
      const result = await h.getDocumentMetadata(documentUri);
      return result as DocumentMetadata;
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * Rename a directory.
   *
   * Re-encrypts the directory metadata with the new name.
   *
   * @param directoryUri - URI of the directory to rename.
   * @param newName - New display name.
   */
  async renameDirectory(directoryUri: string, newName: string): Promise<MutationResult> {
    const h = this.requireHandle();
    try {
      const result = await h.renameDirectory(directoryUri, newName);
      return result as MutationResult;
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * Recursively delete a directory and all its contents.
   *
   * Deletes all documents and subdirectories depth-first.
   *
   * @param directoryUri - URI of the directory to delete.
   * @returns Counts of deleted documents and directories.
   */
  async deleteRecursive(directoryUri: string): Promise<DeleteRecursiveResult> {
    const h = this.requireHandle();
    try {
      const result = await h.deleteRecursive(directoryUri);
      return result as DeleteRecursiveResult;
    } catch (e) {
      throw parseWasmError(e);
    }
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
  async updateMetadata(
    documentUri: string,
    updates: {
      filename?: string;
      description?: string;
      tags?: readonly string[];
    },
  ): Promise<MutationResult> {
    const h = this.requireHandle();
    try {
      const result = await h.updateMetadata(
        documentUri,
        updates.filename ?? null,
        updates.tags ? [...updates.tags] : null,
        updates.description ?? null,
      );
      return result as MutationResult;
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * Replace a document's encrypted content.
   *
   * @param documentUri - URI of the document.
   * @param newContent - New file contents (will be encrypted).
   */
  async updateContent(documentUri: string, newContent: Uint8Array): Promise<MutationResult> {
    const h = this.requireHandle();
    try {
      const result = await h.updateContent(documentUri, newContent);
      return result as MutationResult;
    } catch (e) {
      throw parseWasmError(e);
    }
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
   * @param recipientDid - DID of the recipient.
   * @param recipientPublicKey - Recipient's X25519 public key (32 bytes).
   * @param role - Access role ("read" or "write").
   */
  async share(
    documentUri: string,
    recipientDid: string,
    recipientPublicKey: Uint8Array,
    permissions: string,
    note?: string,
  ): Promise<MutationResult> {
    const h = this.requireHandle();
    try {
      const result = await h.share(documentUri, recipientDid, recipientPublicKey, permissions, note ?? null);
      return result as MutationResult;
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * Revoke a previously created share grant.
   *
   * @param grantUri - URI of the grant record to delete.
   */
  async revokeShare(grantUri: string): Promise<void> {
    const h = this.requireHandle();
    try {
      await h.revokeShare(grantUri);
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  // ---------------------------------------------------------------------------
  // Proposals
  // ---------------------------------------------------------------------------

  /**
   * Whether this FileManager's context is the owner of the workspace/cabinet.
   *
   * Owners apply mutations directly. Non-owners create proposals.
   */
  isOwner(): boolean {
    const h = this.requireHandle();
    try {
      return h.isOwner();
    } catch (e) {
      throw parseWasmError(e);
    }
  }

  /**
   * Sync and apply pending proposals from workspace members.
   *
   * Only meaningful for workspace owners. Returns the number of proposals applied.
   */
  async syncAndApplyProposals(): Promise<number> {
    const h = this.requireHandle();
    try {
      return await h.syncAndApplyProposals();
    } catch (e) {
      throw parseWasmError(e);
    }
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
    if (this.handle) {
      this.handle.free();
      this.handle = null;
    }
  }

  /** TC39 Explicit Resource Management support. */
  [Symbol.dispose](): void {
    this.dispose();
  }

  private requireHandle(): WasmFileManager {
    if (!this.handle) {
      throw parseWasmError(new Error("FileManager has been disposed"));
    }
    return this.handle;
  }
}
