// Device pairing — all operations go through the WASM worker.
// No raw XRPC calls, no manual crypto, no private key access.

import { getOpakeWorker } from "@/lib/worker";
import type { Identity } from "@/lib/storageTypes";
import { base64ToUint8Array } from "@/lib/encoding";
import { rkeyFromUri } from "@/lib/atUri";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface AtBytes {
  readonly $bytes: string;
}

export interface PendingPairRequest {
  readonly uri: string;
  readonly fingerprint: string;
  readonly createdAt: string;
  readonly ephemeralKey: Uint8Array;
}

/** Full pair response record from the PDS — passed as-is to WASM for deserialization. */
export type PairResponseRecord = Record<string, unknown>;

const MAX_KEY_FINGERPRINT = 8;

function formatFingerprint(keyBytes: Uint8Array): string {
  return [...keyBytes.slice(0, MAX_KEY_FINGERPRINT)]
    .map((b) => b.toString(16).padStart(2, "0"))
    .join(":");
}

// ---------------------------------------------------------------------------
// Create pair request (new device)
// ---------------------------------------------------------------------------

export interface CreatePairRequestResult {
  readonly uri: string;
  readonly rkey: string;
  readonly ephemeral_public_key: Uint8Array;
  readonly ephemeral_private_key: Uint8Array;
}

export async function createPairRequest(): Promise<CreatePairRequestResult> {
  const worker = getOpakeWorker();
  return (await worker.createPairRequest()) as CreatePairRequestResult;
}

// ---------------------------------------------------------------------------
// List pending pair requests (existing device)
// ---------------------------------------------------------------------------

export async function listPairRequests(maxAge: number): Promise<PendingPairRequest[]> {
  const worker = getOpakeWorker();
  const entries = (await worker.listPairRequests()) as {
    uri: string;
    value: { ephemeralKey: AtBytes; createdAt: string };
  }[];

  return entries
    .filter((rec) => Date.now() - Date.parse(rec.value.createdAt) < maxAge)
    .map((rec) => {
      const keyBytes = base64ToUint8Array(rec.value.ephemeralKey.$bytes);
      return {
        uri: rec.uri,
        fingerprint: formatFingerprint(keyBytes),
        createdAt: rec.value.createdAt,
        ephemeralKey: keyBytes,
      };
    });
}

// ---------------------------------------------------------------------------
// Poll for pair response (new device)
// ---------------------------------------------------------------------------

export async function pollForPairResponse(
  requestRkey: string,
  did: string,
): Promise<PairResponseRecord | null> {
  const worker = getOpakeWorker();
  const entries = (await worker.listPairResponses()) as {
    uri: string;
    value: {
      request: string;
      wrappedKey: unknown;
      ciphertext: AtBytes;
      nonce: AtBytes;
    };
  }[];

  const requestUri = `at://${did}/app.opake.pairRequest/${requestRkey}`;
  const match = entries.find((rec) => rec.value.request === requestUri);
  if (!match) return null;

  return match.value as PairResponseRecord;
}

// ---------------------------------------------------------------------------
// Receive pair response (new device — core handles unwrap + decrypt)
// ---------------------------------------------------------------------------

export async function receivePairResponse(
  response: PairResponseRecord,
  ephemeralPrivKey: Uint8Array,
): Promise<Identity> {
  const worker = getOpakeWorker();
  return (await worker.receivePairResponse(response, ephemeralPrivKey)) as Identity;
}

// ---------------------------------------------------------------------------
// Approve pair request (existing device — core handles encrypt + wrap)
// ---------------------------------------------------------------------------

export async function approvePairRequest(
  requestUri: string,
  ephemeralPubKey: Uint8Array,
): Promise<void> {
  const worker = getOpakeWorker();
  await worker.approvePairRequest(requestUri, ephemeralPubKey);
}

// ---------------------------------------------------------------------------
// Cleanup (delete request + response records)
// ---------------------------------------------------------------------------

export async function cleanupPairRecords(
  requestUri: string,
  responseRkey: string | null,
): Promise<void> {
  const worker = getOpakeWorker();
  const requestRkey = rkeyFromUri(requestUri);
  await worker.cleanupPairRecords(requestRkey, responseRkey ?? "");
}
