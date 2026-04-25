// Device pairing — thin wrappers over @opake/sdk.
//
// New-device flow (`createPairRequest` / `awaitPairCompletion` / `cancel`)
// lives entirely in the SDK as static methods that take Storage + DID —
// it can't use `getOpake()` because there's no Opake handle yet on the
// new device. The existing-device helpers below go through the handle.

import { getOpake, useAuthStore } from "@/stores/auth";
import { getStorage } from "@/stores/auth";
import { formatFingerprint } from "@/lib/encoding";
import { Opake } from "@opake/sdk";
import type { PairRequestResult, AwaitPairOptions } from "@opake/sdk";

// ---------------------------------------------------------------------------
// Types (re-exported for UI consumption)
// ---------------------------------------------------------------------------

export interface PendingPairRequest {
  readonly uri: string;
  readonly fingerprint: string;
  readonly createdAt: string;
  readonly ephemeralKey: Uint8Array;
}

export type { PairRequestResult };

// ---------------------------------------------------------------------------
// New-device flow
// ---------------------------------------------------------------------------

function requireActiveDid(): string {
  const s = useAuthStore.getState().session;
  if (s.status !== "active") throw new Error("no active session");
  return s.did;
}

export async function createPairRequest(): Promise<PairRequestResult> {
  const storage = await getStorage();
  return Opake.createPairRequest(storage, requireActiveDid());
}

export async function awaitPairCompletion(
  requestRkey: string,
  options?: AwaitPairOptions,
): Promise<void> {
  const storage = await getStorage();
  return Opake.awaitPairCompletion(storage, requireActiveDid(), requestRkey, options);
}

export async function cancelPairRequest(requestRkey: string): Promise<void> {
  const storage = await getStorage();
  return Opake.cancelPairRequest(storage, requireActiveDid(), requestRkey);
}

// ---------------------------------------------------------------------------
// Existing-device flow (uses the Opake handle)
// ---------------------------------------------------------------------------

export async function listPairRequests(maxAge: number): Promise<PendingPairRequest[]> {
  const entries = await getOpake().listPairRequests();

  return entries
    .filter((rec) => Date.now() - Date.parse(rec.createdAt) < maxAge)
    .map((rec) => ({
      uri: rec.uri,
      fingerprint: formatFingerprint(rec.ephemeralKey),
      createdAt: rec.createdAt,
      ephemeralKey: rec.ephemeralKey,
    }));
}

export async function approvePairRequest(
  requestUri: string,
  ephemeralPubKey: Uint8Array,
): Promise<void> {
  await getOpake().approvePairRequest(requestUri, ephemeralPubKey);
}
