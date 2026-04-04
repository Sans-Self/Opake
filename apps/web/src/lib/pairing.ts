// Device pairing — thin wrappers over @opake/sdk pairing methods.

import { getOpake } from "@/stores/auth";
import { formatFingerprint } from "@/lib/encoding";
import { rkeyFromUri } from "@/lib/atUri";
import type { PairRequestResult, PairResponseRecord, Identity } from "@opake/sdk";

// ---------------------------------------------------------------------------
// Types (re-exported for UI consumption)
// ---------------------------------------------------------------------------

export interface PendingPairRequest {
  readonly uri: string;
  readonly fingerprint: string;
  readonly createdAt: string;
  readonly ephemeralKey: Uint8Array;
}

export type { PairResponseRecord };

// ---------------------------------------------------------------------------
// Create pair request (new device)
// ---------------------------------------------------------------------------

export async function createPairRequest(): Promise<PairRequestResult> {
  return getOpake().createPairRequest();
}

// ---------------------------------------------------------------------------
// List pending pair requests (existing device)
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

// ---------------------------------------------------------------------------
// Poll for pair response (new device)
// ---------------------------------------------------------------------------

export async function pollForPairResponse(
  requestRkey: string,
  did: string,
): Promise<PairResponseRecord | null> {
  const responses = await getOpake().listPairResponses();
  const requestUri = `at://${did}/app.opake.pairRequest/${requestRkey}`;
  const match = responses.find((r) => r.requestUri === requestUri);
  return match?.value ?? null;
}

// ---------------------------------------------------------------------------
// Receive pair response (new device — SDK handles unwrap + decrypt)
// ---------------------------------------------------------------------------

export async function receivePairResponse(
  response: PairResponseRecord,
  ephemeralPrivKey: Uint8Array,
): Promise<Identity> {
  return getOpake().receivePairResponse(response, ephemeralPrivKey);
}

// ---------------------------------------------------------------------------
// Approve pair request (existing device — SDK handles encrypt + wrap)
// ---------------------------------------------------------------------------

export async function approvePairRequest(
  requestUri: string,
  ephemeralPubKey: Uint8Array,
): Promise<void> {
  await getOpake().approvePairRequest(requestUri, ephemeralPubKey);
}

// ---------------------------------------------------------------------------
// Cleanup (delete request + response records)
// ---------------------------------------------------------------------------

export async function cleanupPairRecords(
  requestUri: string,
  responseRkey: string | null,
): Promise<void> {
  const requestRkey = rkeyFromUri(requestUri);
  await getOpake().cleanupPairRecords(requestRkey, responseRkey ?? "");
}
