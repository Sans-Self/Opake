// Sharing helpers — resolve recipient, create/list/revoke grants.

import type { WrappedKey } from "@/lib/cryptoTypes";
import type { EncryptedMetadataEnvelope } from "@/lib/pdsTypes";
import type { Session } from "@/lib/storageTypes";
import { authenticatedXrpc, authenticatedDeleteRecord, appview } from "@/lib/api";
import { resolveHandleToPds } from "@/lib/oauth";
import { getCryptoWorker } from "@/lib/worker";
import { base64ToUint8Array, uint8ArrayToBase64 } from "@/lib/encoding";
import { IndexedDbStorage } from "@/lib/indexeddbStorage";

const GRANT_COLLECTION = "app.opake.grant";
const PUBLIC_KEY_COLLECTION = "app.opake.publicKey";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface GrantRecord {
  readonly opakeVersion: number;
  readonly document: string;
  readonly recipient: string;
  readonly wrappedKey: WrappedKey;
  readonly encryptedMetadata: EncryptedMetadataEnvelope;
  readonly expiresAt?: string;
  readonly createdAt: string;
}

export interface GrantEntry {
  readonly uri: string;
  readonly cid: string;
  readonly record: GrantRecord;
}

export interface RecipientInfo {
  readonly did: string;
  readonly pdsUrl: string;
  readonly publicKey: Uint8Array;
}

/** Inbox grant item from the AppView. */
export interface InboxGrantItem {
  readonly uri: string;
  readonly ownerDid: string;
  readonly documentUri: string;
  readonly createdAt: string;
}

// ---------------------------------------------------------------------------
// Recipient resolution
// ---------------------------------------------------------------------------

/** Resolve a handle to a DID + PDS URL + X25519 public key. */
export async function resolveRecipient(handle: string): Promise<RecipientInfo> {
  const { did, pdsUrl } = await resolveHandleToPds(handle);

  const base = pdsUrl.replace(/\/$/, "");
  const url = `${base}/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did)}&collection=${encodeURIComponent(PUBLIC_KEY_COLLECTION)}&rkey=self`;
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`No Opake public key found for ${handle} (${did})`);
  }

  const record = (await response.json()) as {
    value?: { publicKey?: { $bytes: string } | string };
  };
  const raw = record.value?.publicKey;
  if (!raw) {
    throw new Error(`No encryption public key published for ${handle}`);
  }

  const b64 = typeof raw === "string" ? raw : raw.$bytes;
  const publicKey = base64ToUint8Array(b64);

  return { did, pdsUrl, publicKey };
}

// ---------------------------------------------------------------------------
// Grant creation
// ---------------------------------------------------------------------------

interface CreateGrantParams {
  readonly pdsUrl: string;
  readonly ownerDid: string;
  readonly documentUri: string;
  readonly recipientDid: string;
  readonly contentKey: Uint8Array;
  readonly recipientPublicKey: Uint8Array;
  readonly session: Session;
}

/** Wrap the content key to the recipient and create a grant record on the PDS. */
export async function createGrant(params: CreateGrantParams): Promise<string> {
  const worker = getCryptoWorker();

  const wrappedKey = await worker.wrapKey(
    params.contentKey,
    params.recipientPublicKey,
    params.recipientDid,
  );

  const metadata = { permissions: "read", note: null };
  const metadataBytes = new TextEncoder().encode(JSON.stringify(metadata));
  const encryptedMeta = await worker.encryptBlob(params.contentKey, metadataBytes);

  const record = {
    $type: GRANT_COLLECTION,
    opakeVersion: await worker.schemaVersion(),
    document: params.documentUri,
    recipient: params.recipientDid,
    wrappedKey,
    encryptedMetadata: {
      ciphertext: { $bytes: uint8ArrayToBase64(encryptedMeta.ciphertext) },
      nonce: { $bytes: uint8ArrayToBase64(encryptedMeta.nonce) },
    },
    createdAt: new Date().toISOString(),
  };

  const result = (await authenticatedXrpc(
    {
      pdsUrl: params.pdsUrl,
      lexicon: "com.atproto.repo.createRecord",
      method: "POST",
      body: {
        repo: params.ownerDid,
        collection: GRANT_COLLECTION,
        record,
      },
    },
    params.session,
  )) as { uri: string };

  return result.uri;
}

// ---------------------------------------------------------------------------
// Grant listing (outgoing — from own PDS)
// ---------------------------------------------------------------------------

interface ListRecordsResponse {
  readonly records: readonly { uri: string; cid: string; value: GrantRecord }[];
  readonly cursor?: string;
}

/** List all outgoing grants from the owner's PDS. */
export async function listOutgoingGrants(
  pdsUrl: string,
  did: string,
  session: Session,
): Promise<GrantEntry[]> {
  /* eslint-disable functional/no-loop-statements, functional/no-let, functional/immutable-data, functional/prefer-immutable-types -- paginated cursor loop */
  const entries: GrantEntry[] = [];
  let cursor: string | undefined;

  do {
    const params = new URLSearchParams({
      repo: did,
      collection: GRANT_COLLECTION,
      limit: "100",
    });
    if (cursor) params.set("cursor", cursor);

    const response = (await authenticatedXrpc(
      { pdsUrl, lexicon: `com.atproto.repo.listRecords?${params}` },
      session,
    )) as ListRecordsResponse;

    for (const r of response.records) {
      entries.push({ uri: r.uri, cid: r.cid, record: r.value });
    }

    cursor = response.cursor;
  } while (cursor);
  /* eslint-enable functional/no-loop-statements, functional/no-let, functional/immutable-data, functional/prefer-immutable-types */

  return entries;
}

// ---------------------------------------------------------------------------
// Grant listing (incoming — from AppView inbox)
// ---------------------------------------------------------------------------

interface InboxResponse {
  readonly grants: InboxGrantItem[];
  readonly cursor?: string;
}

/** Fetch incoming grants from the AppView inbox. */
export async function listIncomingGrants(did: string): Promise<InboxGrantItem[]> {
  const storage = new IndexedDbStorage();
  const config = await storage.loadConfig().catch(() => null);
  const appviewUrl = config?.appviewUrl;
  if (!appviewUrl) return [];

  /* eslint-disable functional/no-loop-statements, functional/no-let, functional/immutable-data, functional/prefer-immutable-types -- paginated cursor loop */
  const items: InboxGrantItem[] = [];
  let cursor: string | undefined;

  do {
    const params = new URLSearchParams({ did, limit: "100" });
    if (cursor) params.set("cursor", cursor);

    const response = (await appview(`/api/inbox?${params}`, {
      pdsUrl: "",
      appviewUrl,
    })) as InboxResponse;

    items.push(...response.grants);
    cursor = response.cursor;
  } while (cursor);
  /* eslint-enable functional/no-loop-statements, functional/no-let, functional/immutable-data, functional/prefer-immutable-types */

  return items;
}

// ---------------------------------------------------------------------------
// Grant revocation
// ---------------------------------------------------------------------------

/** Delete a grant record by AT-URI. */
export async function revokeGrant(
  pdsUrl: string,
  did: string,
  grantUri: string,
  session: Session,
): Promise<void> {
  const segments = grantUri.split("/");
  const rkey = segments[segments.length - 1];

  await authenticatedDeleteRecord({ pdsUrl, did, collection: GRANT_COLLECTION, rkey }, session);
}
