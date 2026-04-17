// Structured error types for Opake operations.
//
// WASM errors arrive as strings in the format "Kind: message". The SDK
// parses them into OpakeError instances with a typed `.kind` discriminant,
// so consumers can match on specific failure modes.

/** Error kinds matching opake-core's Error enum variants. */
export type OpakeErrorKind =
  | "NotFound"
  | "RecipientNotReady"
  | "Auth"
  | "Encryption"
  | "Decryption"
  | "KeyWrap"
  | "InvalidRecord"
  | "Storage"
  | "Xrpc"
  | "Appview"
  | "AlreadyExists"
  | "AmbiguousName"
  | "Serialization"
  | "Mnemonic"
  | "Sse"
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
  "RecipientNotReady",
  "Auth",
  "Encryption",
  "Decryption",
  "KeyWrap",
  "InvalidRecord",
  "Storage",
  "Xrpc",
  "Appview",
  "AlreadyExists",
  "AmbiguousName",
  "Serialization",
  "Mnemonic",
  "Sse",
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
