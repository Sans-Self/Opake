// Structured error types for Opake operations.
//
// WASM errors arrive as strings in the format "Kind: message". The SDK
// parses them into OpakeError instances with a typed `.kind` discriminant,
// so consumers can match on specific failure modes.

/** Error kinds matching opake-core's Error enum variants. */
export type OpakeErrorKind =
  | "NotFound"
  | "IdentityMissing"
  | "RecipientNotReady"
  | "Auth"
  | "Encryption"
  | "Decryption"
  | "KeyWrap"
  | "InvalidRecord"
  | "Storage"
  | "Xrpc"
  | "Indexer"
  | "AlreadyExists"
  | "AmbiguousName"
  | "Serialization"
  | "Mnemonic"
  | "Sse"
  // A workspace-scoped indexer call found no keyring chain head for the
  // workspace. Ambiguous by construction between a genesis still in flight and
  // a torn-down chain, so copy for it must claim neither deletion nor lag — the
  // honest surface is "the indexer cannot answer for this workspace", and the
  // next bootstrap or keyring event resolves which case it was. The client
  // retries this within the visibility window before it ever reaches a caller.
  | "WorkspaceNotIndexed"
  // A keyring's declared identity failed the derivation check: its genesis rkey
  // is not derived from the key material it carries, under the declared owner
  // DID. A forged or malformed workspace-identity claim; nothing is adopted. On
  // listing surfaces such a record is dropped silently — surfacing it would only
  // inform a forger — so a caller sees this only on a direct resolve.
  | "WorkspaceIdentityMismatch"
  // The indexer read an indexed chain head and the caller's DID is absent from
  // its members. Definitive, never retried: the UI may say "not permitted".
  | "NotWorkspaceMember"
  // The indexer had not caught up with a prior own-write within the bounded
  // retry window — the pipeline is behind, distinct from an authorization
  // denial. Lets the UI say "still syncing" rather than "not permitted".
  | "VisibilityTimeout"
  // A conditional (compare-and-swap) write lost the race — the record's CID
  // moved before the write landed. For background maintenance this is "another
  // runner finished first", not a failure; the runner re-derives and skips.
  | "CasConflict"
  | "Unknown";

/**
 * Structured error from an Opake operation.
 *
 * @example
 * ```typescript
 * try {
 *   await cabinet.download(uri);
 * } catch (e) {
 *   if (e instanceof OpakeError && e.kind === "NotFound") {
 *     console.log("Document not found:", e.message);
 *   }
 * }
 * ```
 */
export class OpakeError extends Error {
  readonly kind: OpakeErrorKind;

  constructor(kind: OpakeErrorKind, message: string) {
    super(message);
    this.name = "OpakeError";
    this.kind = kind;
  }
}

const KNOWN_KINDS = new Set<string>([
  "NotFound",
  "IdentityMissing",
  "RecipientNotReady",
  "Auth",
  "Encryption",
  "Decryption",
  "KeyWrap",
  "InvalidRecord",
  "Storage",
  "Xrpc",
  "Indexer",
  "AlreadyExists",
  "AmbiguousName",
  "Serialization",
  "Mnemonic",
  "Sse",
  "WorkspaceNotIndexed",
  "WorkspaceIdentityMismatch",
  "NotWorkspaceMember",
  "VisibilityTimeout",
  "CasConflict",
]);

/**
 * Parse a WASM error string into a structured OpakeError.
 *
 * Expected format: `"Kind: rest of the message"`.
 * Falls back to `Unknown` if the format doesn't match.
 */
/** Decorator: catch WASM errors and rethrow as typed OpakeError. Works on sync and async methods. */
export function wrapWasmErrors(_target: any, _context: ClassMethodDecoratorContext) {
  return function (this: unknown, ...args: any[]): any {
    try {
      const result = _target.call(this, ...args);
      if (result instanceof Promise)
        return result.catch((e: unknown) => {
          throw parseWasmError(e);
        });
      return result;
    } catch (e) {
      throw parseWasmError(e);
    }
  };
}

export function parseWasmError(error: unknown): OpakeError {
  const message = error instanceof Error ? error.message : String(error);

  const colonIndex = message.indexOf(": ");
  if (colonIndex > 0) {
    const prefix = message.slice(0, colonIndex);
    if (KNOWN_KINDS.has(prefix)) {
      return new OpakeError(prefix as OpakeErrorKind, message.slice(colonIndex + 2));
    }
  }

  return new OpakeError("Unknown", message);
}
