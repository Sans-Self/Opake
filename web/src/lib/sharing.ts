// Sharing helpers — resolve recipient, create/list/revoke grants.

import type { WrappedKey } from "@/lib/cryptoTypes";
import type {
  AccountConfigRecord,
  EncryptedMetadataEnvelope,
  DocumentRecord,
  DocumentMetadata,
} from "@/lib/pdsTypes";
import type { DecryptedBlob } from "@/lib/preview";
import type { Session } from "@/lib/storageTypes";
import {
  authenticatedXrpc,
  authenticatedGetRecord,
  authenticatedDeleteRecord,
  authenticatedAppview,
} from "@/lib/api";
import { resolveHandleToPds } from "@/lib/oauth";
import { pdsUrlFromDid } from "@/lib/did";
import { getCryptoWorker } from "@/lib/worker";
import { base64ToUint8Array, uint8ArrayToBase64 } from "@/lib/encoding";
import { rkeyFromUri } from "@/lib/atUri";
import { triggerBrowserDownload } from "@/lib/download";
import { decryptEnvelope } from "@/stores/documents/decrypt";
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
    throw new Error(`${handle} hasn't signed in to Opake yet — they need to log in at least once`);
  }

  const record = (await response.json()) as {
    value?: { publicKey?: { $bytes: string } | string };
  };
  const raw = record.value?.publicKey;
  if (!raw) {
    throw new Error(`${handle} hasn't signed in to Opake yet — they need to log in at least once`);
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

/** Fetch incoming grants from the AppView inbox (authenticated). */
export async function listIncomingGrants(
  pdsUrl: string,
  did: string,
  session: Session,
  signingKey: Uint8Array,
): Promise<InboxGrantItem[]> {
  const worker = getCryptoWorker();
  const [collection, rkey] = await Promise.all([
    worker.accountConfigCollection(),
    worker.accountConfigRkey(),
  ]);
  const accountConfig = await authenticatedGetRecord<AccountConfigRecord>(
    { pdsUrl, did, collection, rkey },
    session,
  ).catch(() => null);
  const appviewUrl =
    accountConfig?.value.appviewUrl ??
    (import.meta.env.VITE_APPVIEW_URL as string | undefined) ??
    "https://appview.opake.app";

  /* eslint-disable functional/no-loop-statements, functional/no-let, functional/immutable-data, functional/prefer-immutable-types -- paginated cursor loop */
  const items: InboxGrantItem[] = [];
  let cursor: string | undefined;

  do {
    const query = new URLSearchParams({ did, limit: "100" });
    if (cursor) query.set("cursor", cursor);

    const response = (await authenticatedAppview({
      appviewUrl,
      path: `/api/inbox?${query}`,
      did,
      signingKey,
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
  await authenticatedDeleteRecord(
    { pdsUrl, did, collection: GRANT_COLLECTION, rkey: rkeyFromUri(grantUri) },
    session,
  );
}

// ---------------------------------------------------------------------------
// Incoming grant resolution (fetch record → unwrap → decrypt metadata)
// ---------------------------------------------------------------------------

const DOCUMENT_COLLECTION = "app.opake.document";

/** Resolved incoming grant with decrypted document metadata. */
export interface ResolvedIncomingGrant extends InboxGrantItem {
  readonly ownerPdsUrl: string;
  readonly contentKey: Uint8Array;
  readonly documentRecord: DocumentRecord;
  readonly metadata: DocumentMetadata;
}

/** Fetch a record from a PDS without authentication (records are public in atproto). */
async function publicGetRecord(
  pdsUrl: string,
  repo: string,
  collection: string,
  rkey: string,
): Promise<{ uri: string; cid: string; value: unknown }> {
  const params = new URLSearchParams({ repo, collection, rkey });
  const response = await fetch(
    `${pdsUrl.replace(/\/$/, "")}/xrpc/com.atproto.repo.getRecord?${params}`,
  );
  if (!response.ok) throw new Error(`getRecord failed: HTTP ${response.status}`);
  return response.json() as Promise<{ uri: string; cid: string; value: unknown }>;
}

/**
 * Resolve an incoming grant: fetch the full grant + document records from the
 * owner's PDS, unwrap the content key, and decrypt the document metadata.
 */
export async function resolveIncomingGrant(
  grant: InboxGrantItem,
  privateKey: Uint8Array,
  knownPdsUrl?: string,
): Promise<ResolvedIncomingGrant> {
  const ownerPdsUrl = knownPdsUrl ?? (await pdsUrlFromDid(grant.ownerDid));

  // Fetch the grant record to get the wrappedKey
  const grantResult = await publicGetRecord(
    ownerPdsUrl,
    grant.ownerDid,
    GRANT_COLLECTION,
    rkeyFromUri(grant.uri),
  );
  const grantRecord = grantResult.value as GrantRecord;

  // Unwrap the content key with our private key
  const worker = getCryptoWorker();
  const contentKey = await worker.unwrapKey(grantRecord.wrappedKey, privateKey);

  // Fetch the document record to decrypt its metadata
  const docResult = await publicGetRecord(
    ownerPdsUrl,
    grant.ownerDid,
    DOCUMENT_COLLECTION,
    rkeyFromUri(grantRecord.document),
  );
  const documentRecord = docResult.value as DocumentRecord;

  // Decrypt document metadata using the content key
  const { ciphertext, nonce } = decryptEnvelope(documentRecord.encryptedMetadata);
  const metadata = await worker.decryptMetadata(contentKey, ciphertext, nonce);

  return {
    ...grant,
    ownerPdsUrl,
    contentKey,
    documentRecord,
    metadata,
  };
}

/** Fetch and decrypt the blob of a resolved incoming grant. */
async function decryptIncomingBlob(resolved: ResolvedIncomingGrant): Promise<DecryptedBlob> {
  const { encryption } = resolved.documentRecord;
  if (encryption.$type !== "app.opake.document#directEncryption") {
    throw new Error("Keyring-encrypted documents are not yet supported");
  }

  const cid = resolved.documentRecord.blob.ref.$link;
  const blobUrl = `${resolved.ownerPdsUrl.replace(/\/$/, "")}/xrpc/com.atproto.sync.getBlob?did=${encodeURIComponent(resolved.ownerDid)}&cid=${encodeURIComponent(cid)}`;
  const blobResponse = await fetch(blobUrl);
  if (!blobResponse.ok) throw new Error(`getBlob failed: HTTP ${blobResponse.status}`);

  const worker = getCryptoWorker();
  const blobNonce = base64ToUint8Array(encryption.envelope.nonce.$bytes);
  const plaintext = await worker.decryptBlob(
    resolved.contentKey,
    new Uint8Array(await blobResponse.arrayBuffer()),
    blobNonce,
  );

  return { plaintext, metadata: resolved.metadata };
}

/** Download a resolved incoming grant's blob to the user's device. */
export async function downloadIncomingGrant(resolved: ResolvedIncomingGrant): Promise<void> {
  const { plaintext, metadata } = await decryptIncomingBlob(resolved);
  triggerBrowserDownload(plaintext, metadata.name, metadata.mimeType ?? "application/octet-stream");
}

/**
 * Create a decrypt function for a shared incoming document.
 * Suitable for passing directly to `<FilePreview decrypt={...} />`.
 */
export function decryptIncomingDocument(
  resolved: ResolvedIncomingGrant,
): () => Promise<DecryptedBlob> {
  return () => decryptIncomingBlob(resolved);
}
