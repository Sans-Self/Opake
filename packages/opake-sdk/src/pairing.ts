// Device pairing — typed wrappers around the WASM bindings.
//
// Split by device role:
//
// - **New device** (no identity yet): standalone functions that take the
//   caller's Storage directly. `createPairRequest` persists the ephemeral
//   private key inside WASM; `awaitPairCompletion` polls until the paired
//   device responds, decrypts the identity, and writes it to Storage.
//   JS never sees the private key or the Identity DTO.
//
// - **Existing device** (has identity, holds an Opake handle): instance
//   methods on `Opake` that wrap the WASM handle's corresponding methods.

import type { Storage } from "./storage";
import type { PairCompletionResult, PendingPairRequest, PairRequestResult } from "./types";
import { pairRequestResultSchema } from "./schemas";
import { createStorageAdapter } from "./storage-adapter";
import { initWasm } from "./wasm";

type Ctx = import("../wasm/opake.js").OpakeContext;

function base64ToBytes(b64: string): Uint8Array {
  const padded = b64 + "=".repeat((4 - (b64.length % 4)) % 4);
  const binary = atob(padded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

// ---------------------------------------------------------------------------
// New-device flow (identity-less — takes Storage directly)
// ---------------------------------------------------------------------------

/** Options for {@link awaitPairCompletion}. */
export interface AwaitPairOptions {
  /** Milliseconds between polls (default 3000). */
  readonly pollIntervalMs?: number;
  /** Abort polling after this many ms (default: none). */
  readonly timeoutMs?: number;
  /** Optional cancellation signal. Aborting rejects the promise. */
  readonly signal?: AbortSignal;
}

export async function createPairRequest(
  storage: Storage,
  did: string,
): Promise<PairRequestResult> {
  const wasm = await initWasm();
  const adapter = createStorageAdapter(storage);
  const result = await wasm.createPairRequest(did, adapter);
  return pairRequestResultSchema.parse(result);
}

export async function awaitPairCompletion(
  storage: Storage,
  did: string,
  requestRkey: string,
  options: AwaitPairOptions = {},
): Promise<PairCompletionResult> {
  const wasm = await initWasm();
  const interval = options.pollIntervalMs ?? 3000;
  const deadline =
    options.timeoutMs === undefined ? undefined : Date.now() + options.timeoutMs;

  while (true) {
    if (options.signal?.aborted) {
      throw new DOMException("pairing cancelled", "AbortError");
    }

    const adapter = createStorageAdapter(storage);
    const completion = (await wasm.tryCompletePair(
      did,
      adapter,
      requestRkey,
    )) as PairCompletionResult;
    if (completion.completed) return completion;

    if (deadline !== undefined && Date.now() >= deadline) {
      throw new Error("pairing timed out before the other device responded");
    }

    // Honour a pending abort promptly rather than sleeping through it.
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(resolve, interval);
      options.signal?.addEventListener(
        "abort",
        () => {
          clearTimeout(timer);
          reject(new DOMException("pairing cancelled", "AbortError"));
        },
        { once: true },
      );
    });
  }
}

export async function cancelPairRequest(
  storage: Storage,
  did: string,
  requestRkey: string,
): Promise<void> {
  const wasm = await initWasm();
  const adapter = createStorageAdapter(storage);
  await wasm.cancelPairRequest(did, adapter, requestRkey);
}

// ---------------------------------------------------------------------------
// Existing-device flow (delegates through an Opake handle)
// ---------------------------------------------------------------------------

export function listPairRequests(ctx: Ctx): Promise<readonly PendingPairRequest[]> {
  return ctx.listPairRequests().then(
    (
      entries: readonly {
        uri: string;
        value: {
          x25519EphemeralKey: { $bytes: string };
          mlKemEphemeralKey: { $bytes: string };
          createdAt: string;
        };
      }[],
    ) =>
      entries.map((e) => ({
        uri: e.uri,
        x25519EphemeralKey: base64ToBytes(e.value.x25519EphemeralKey.$bytes),
        mlKemEphemeralKey: base64ToBytes(e.value.mlKemEphemeralKey.$bytes),
        createdAt: e.value.createdAt,
      })),
  );
}

export function approvePairRequest(
  ctx: Ctx,
  requestUri: string,
  x25519EphemeralPublicKey: Uint8Array,
  mlKemEphemeralPublicKey: Uint8Array,
): Promise<void> {
  return ctx.approvePairRequest(
    requestUri,
    x25519EphemeralPublicKey,
    mlKemEphemeralPublicKey,
  );
}

export function cleanupExpiredPairRequests(ctx: Ctx): Promise<number> {
  return ctx.cleanupExpiredPairRequests();
}
