// Sharing helpers — resolve recipient, create/list/revoke grants.

import type { WrappedKey } from "@/lib/cryptoTypes";
import type { EncryptedMetadataEnvelope, DocumentRecord, DocumentMetadata } from "@/lib/pdsTypes";
import type { DecryptedBlob } from "@/lib/preview";
import type { Session } from "@/lib/storageTypes";
import { authenticatedXrpc, authenticatedDeleteRecord, DEFAULT_APPVIEW_URL } from "@/lib/api";
import { resolveHandleToPds } from "@/lib/oauth";
import { pdsUrlFromDid } from "@/lib/did";
import { getOpakeWorker } from "@/lib/worker";
import { base64ToUint8Array, uint8ArrayToBase64 } from "@/lib/encoding";
import { rkeyFromUri } from "@/lib/atUri";
import { formatRelativeDate, mimeTypeToFileType, formatFileSize } from "@/lib/format";
import { triggerBrowserDownload } from "@/lib/download";
import type { FileItem } from "@/components/cabinet/types";
import { decryptEnvelope } from "@/stores/documents/decrypt";
import { GrantRecordSchema, DocumentRecordSchema, ListRecordsResponseSchema } from "@/lib/schemas";

const GrantListResponseSchema = ListRecordsResponseSchema(GrantRecordSchema);
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
// Grant → FileItem conversion
// ---------------------------------------------------------------------------

/** Build a FileItem from an incoming grant, optionally with resolved metadata. */
export function incomingGrantToFileItem(
  grant: InboxGrantItem,
  ownerDisplay: string,
  resolved?: ResolvedIncomingGrant,
): FileItem {
  return {
    id: grant.uri,
    uri: grant.uri,
    name: resolved?.metadata.name ?? "Shared file",
    kind: "file",
    fileType: resolved?.metadata.mimeType
      ? mimeTypeToFileType(resolved.metadata.mimeType)
      : undefined,
    mimeType: resolved?.metadata.mimeType ?? undefined,
    size: resolved?.metadata.size != null ? formatFileSize(resolved.metadata.size) : undefined,
    encrypted: true,
    status: "shared",
    modified: formatRelativeDate(grant.createdAt),
    decrypted: resolved !== undefined,
    tags: [],
    subtitle: `from ${ownerDisplay}`,
  };
}

// ---------------------------------------------------------------------------
// Recipient resolution
// ---------------------------------------------------------------------------

/** Thrown when the recipient exists on atproto but hasn't set up Opake yet. */
export class RecipientNotReadyError extends Error {
  readonly recipientDid: string;
  readonly recipientHandle: string;

  constructor(handle: string, did: string) {
    super(`${handle} hasn't set up Opake yet — they need to log in on any device first`);
    this.name = "RecipientNotReadyError";
    this.recipientDid = did;
    this.recipientHandle = handle;
  }
}

/** Resolve a handle to a DID + PDS URL + X25519 public key. */
export async function resolveRecipient(handle: string): Promise<RecipientInfo> {
  const { did, pdsUrl } = await resolveHandleToPds(handle);

  const base = pdsUrl.replace(/\/$/, "");
  const url = `${base}/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did)}&collection=${encodeURIComponent(PUBLIC_KEY_COLLECTION)}&rkey=self`;
  const response = await fetch(url);
  if (!response.ok) {
    throw new RecipientNotReadyError(handle, did);
  }

  const record = (await response.json()) as {
    value?: { publicKey?: { $bytes: string } | string };
  };
  const raw = record.value?.publicKey;
  if (!raw) {
    throw new RecipientNotReadyError(handle, did);
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

/** Encrypt grant/pending-share metadata (permissions + note) with a content key. */
async function encryptShareMetadata(
  worker: Awaited<ReturnType<typeof getOpakeWorker>>,
  contentKey: Uint8Array,
): Promise<{ ciphertext: { $bytes: string }; nonce: { $bytes: string } }> {
  const metadata = { permissions: "read", note: null };
  const metadataBytes = new TextEncoder().encode(JSON.stringify(metadata));
  const encryptedMeta = await worker.encryptBlob(contentKey, metadataBytes);
  return {
    ciphertext: { $bytes: uint8ArrayToBase64(encryptedMeta.ciphertext) },
    nonce: { $bytes: uint8ArrayToBase64(encryptedMeta.nonce) },
  };
}

/** Wrap the content key to the recipient and create a grant record on the PDS. */
export async function createGrant(params: CreateGrantParams): Promise<string> {
  const worker = getOpakeWorker();

  const wrappedKey = await worker.wrapKey(
    params.contentKey,
    params.recipientPublicKey,
    params.recipientDid,
  );

  const encryptedMetadata = await encryptShareMetadata(worker, params.contentKey);

  const record = {
    $type: GRANT_COLLECTION,
    opakeVersion: await worker.schemaVersion(),
    document: params.documentUri,
    recipient: params.recipientDid,
    wrappedKey,
    encryptedMetadata,
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
// Pending share creation (recipient not ready)
// ---------------------------------------------------------------------------

const PENDING_SHARE_COLLECTION = "app.opake.pendingShare";

interface CreatePendingShareParams {
  readonly pdsUrl: string;
  readonly ownerDid: string;
  readonly documentUri: string;
  readonly recipient: string;
  readonly contentKey: Uint8Array;
  readonly session: Session;
}

/** Create a pendingShare record when the recipient hasn't set up Opake yet. */
export async function createPendingShare(params: CreatePendingShareParams): Promise<string> {
  const worker = getOpakeWorker();

  const encryptedMetadata = await encryptShareMetadata(worker, params.contentKey);

  const record = {
    $type: PENDING_SHARE_COLLECTION,
    opakeVersion: await worker.schemaVersion(),
    document: params.documentUri,
    recipient: params.recipient,
    encryptedMetadata,
    createdAt: new Date().toISOString(),
  };

  const result = (await authenticatedXrpc(
    {
      pdsUrl: params.pdsUrl,
      lexicon: "com.atproto.repo.createRecord",
      method: "POST",
      body: {
        repo: params.ownerDid,
        collection: PENDING_SHARE_COLLECTION,
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

    const response = GrantListResponseSchema.parse(
      await authenticatedXrpc(
        { pdsUrl, lexicon: `com.atproto.repo.listRecords?${params}` },
        session,
      ),
    );

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

/** Fetch incoming grants from the AppView inbox via WASM. */
export async function listIncomingGrants(
  pdsUrl: string,
  did: string,
  session: Session,
  signingKey: Uint8Array,
): Promise<InboxGrantItem[]> {
  const worker = getOpakeWorker();
  const result = await worker.fetchIncomingGrants(
    pdsUrl,
    session,
    signingKey,
    did,
    DEFAULT_APPVIEW_URL,
  );
  return result.grants as InboxGrantItem[];
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
  const grantRecord = GrantRecordSchema.parse(grantResult.value);

  // Unwrap the content key with our private key
  const worker = getOpakeWorker();
  const contentKey = await worker.unwrapKey(grantRecord.wrappedKey, privateKey);

  // Fetch the document record to decrypt its metadata
  const docResult = await publicGetRecord(
    ownerPdsUrl,
    grant.ownerDid,
    DOCUMENT_COLLECTION,
    rkeyFromUri(grantRecord.document),
  );
  const documentRecord = DocumentRecordSchema.parse(docResult.value);

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

  const worker = getOpakeWorker();
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
