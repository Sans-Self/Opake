// Device pairing XRPC orchestration.
// Consumes authenticatedXrpc from api.ts and crypto worker functions.

import type { Remote } from "comlink";
import type { CryptoApi } from "@/workers/crypto.worker";
import type { WrappedKey, AtBytes } from "@/lib/crypto-types";
import type { Identity, Session } from "@/lib/storage-types";
import { authenticatedXrpc } from "@/lib/api";
import {
  uint8ArrayToBase64,
  base64ToUint8Array,
  formatFingerprint,
  rkeyFromUri,
} from "@/lib/encoding";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface PendingPairRequest {
  uri: string;
  fingerprint: string;
  createdAt: string;
  ephemeralKey: Uint8Array;
}

interface PairResponseRecord {
  wrappedKey: WrappedKey;
  ciphertext: AtBytes;
  nonce: AtBytes;
}

const PAIR_REQUEST_COLLECTION = "app.opake.pairRequest";
const PAIR_RESPONSE_COLLECTION = "app.opake.pairResponse";
const SCHEMA_VERSION = 1;

// ---------------------------------------------------------------------------
// Create pair request (new device)
// ---------------------------------------------------------------------------

/** Publish a pairRequest record and return its AT-URI. */
export async function createPairRequest(
  pdsUrl: string,
  did: string,
  ephemeralPubKey: Uint8Array,
  session: Session,
): Promise<string> {
  const rkey = generateTid();

  const record = {
    $type: PAIR_REQUEST_COLLECTION,
    opakeVersion: SCHEMA_VERSION,
    ephemeralKey: { $bytes: uint8ArrayToBase64(ephemeralPubKey) },
    algo: "x25519",
    createdAt: new Date().toISOString(),
  };

  const result = (await authenticatedXrpc(
    {
      pdsUrl,
      lexicon: "com.atproto.repo.createRecord",
      method: "POST",
      body: {
        repo: did,
        collection: PAIR_REQUEST_COLLECTION,
        rkey,
        record,
      },
    },
    session,
  )) as { uri: string };

  return result.uri;
}

// ---------------------------------------------------------------------------
// List pending pair requests (existing device)
// ---------------------------------------------------------------------------

export async function listPairRequests(
  pdsUrl: string,
  did: string,
  session: Session,
): Promise<PendingPairRequest[]> {
  const result = (await authenticatedXrpc(
    {
      pdsUrl,
      lexicon: `com.atproto.repo.listRecords?repo=${encodeURIComponent(did)}&collection=${PAIR_REQUEST_COLLECTION}&limit=50`,
      method: "GET",
    },
    session,
  )) as {
    records: Array<{
      uri: string;
      value: {
        ephemeralKey: AtBytes;
        createdAt: string;
      };
    }>;
  };

  return result.records.map((rec) => {
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
  pdsUrl: string,
  did: string,
  requestRkey: string,
  session: Session,
): Promise<PairResponseRecord | null> {
  const result = (await authenticatedXrpc(
    {
      pdsUrl,
      lexicon: `com.atproto.repo.listRecords?repo=${encodeURIComponent(did)}&collection=${PAIR_RESPONSE_COLLECTION}&limit=50`,
      method: "GET",
    },
    session,
  )) as {
    records: Array<{
      uri: string;
      value: {
        request: string;
        wrappedKey: WrappedKey;
        ciphertext: AtBytes;
        nonce: AtBytes;
      };
    }>;
  };

  // Find the response that references our request
  const requestUri = `at://${did}/${PAIR_REQUEST_COLLECTION}/${requestRkey}`;
  const match = result.records.find((rec) => rec.value.request === requestUri);
  if (!match) return null;

  return {
    wrappedKey: match.value.wrappedKey,
    ciphertext: match.value.ciphertext,
    nonce: match.value.nonce,
  };
}

// ---------------------------------------------------------------------------
// Receive pair response (new device — unwrap + decrypt)
// ---------------------------------------------------------------------------

export async function receivePairResponse(
  response: PairResponseRecord,
  ephemeralPrivKey: Uint8Array,
  worker: Remote<CryptoApi>,
): Promise<Identity> {
  // Unwrap the content key using the ephemeral private key
  const contentKey = await worker.unwrapKey(response.wrappedKey, ephemeralPrivKey);

  // Decrypt the identity JSON
  const ciphertext = base64ToUint8Array(response.ciphertext.$bytes);
  const nonce = base64ToUint8Array(response.nonce.$bytes);
  const plaintext = await worker.decryptBlob(contentKey, ciphertext, nonce);

  // Parse the identity (mirrors Rust's serde_json::from_slice)
  const decoder = new TextDecoder();
  const identity = JSON.parse(decoder.decode(plaintext)) as Identity;

  return identity;
}

// ---------------------------------------------------------------------------
// Approve pair request (existing device — encrypt + wrap)
// ---------------------------------------------------------------------------

export async function approvePairRequest(
  pdsUrl: string,
  did: string,
  requestUri: string,
  ephemeralPubKey: Uint8Array,
  identity: Identity,
  session: Session,
  worker: Remote<CryptoApi>,
): Promise<string> {
  // Generate a content key for encrypting the identity
  const contentKey = await worker.generateContentKey();

  // Serialize identity to JSON (mirrors Rust's serde_json::to_vec)
  const encoder = new TextEncoder();
  const plaintext = encoder.encode(JSON.stringify(identity));

  // Encrypt identity with the content key
  const encrypted = await worker.encryptBlob(contentKey, plaintext);

  // Wrap the content key to the requester's ephemeral public key
  const wrappedKey = await worker.wrapKey(contentKey, ephemeralPubKey, did);

  const rkey = generateTid();

  const record = {
    $type: PAIR_RESPONSE_COLLECTION,
    opakeVersion: SCHEMA_VERSION,
    request: requestUri,
    wrappedKey,
    ciphertext: { $bytes: uint8ArrayToBase64(encrypted.ciphertext) },
    nonce: { $bytes: uint8ArrayToBase64(encrypted.nonce) },
    algo: "aes-256-gcm",
    createdAt: new Date().toISOString(),
  };

  const result = (await authenticatedXrpc(
    {
      pdsUrl,
      lexicon: "com.atproto.repo.createRecord",
      method: "POST",
      body: {
        repo: did,
        collection: PAIR_RESPONSE_COLLECTION,
        rkey,
        record,
      },
    },
    session,
  )) as { uri: string };

  return result.uri;
}

// ---------------------------------------------------------------------------
// Cleanup (delete request + response records)
// ---------------------------------------------------------------------------

export async function cleanupPairRecords(
  pdsUrl: string,
  did: string,
  requestUri: string,
  responseUri: string | null,
  session: Session,
): Promise<void> {
  const deleteRecord = async (collection: string, uri: string) => {
    const rkey = rkeyFromUri(uri);
    await authenticatedXrpc(
      {
        pdsUrl,
        lexicon: "com.atproto.repo.deleteRecord",
        method: "POST",
        body: { repo: did, collection, rkey },
      },
      session,
    );
  };

  await deleteRecord(PAIR_REQUEST_COLLECTION, requestUri);
  if (responseUri) {
    await deleteRecord(PAIR_RESPONSE_COLLECTION, responseUri);
  }
}

// ---------------------------------------------------------------------------
// TID generation (AT Protocol timestamp-based ID)
// ---------------------------------------------------------------------------

const TID_CHARS = "234567abcdefghijklmnopqrstuvwxyz";

function generateTid(): string {
  const now = BigInt(Date.now()) * 1000n;
  const clockId = BigInt(Math.floor(Math.random() * 1024));
  const tid = (now << 10n) | clockId;

  let result = "";
  let remaining = tid;
  for (let i = 0; i < 13; i++) {
    result = TID_CHARS[Number(remaining & 31n)] + result;
    remaining >>= 5n;
  }

  return result;
}
