// Sharing helpers — resolve recipient, create/list/revoke grants.

import type { DocumentMetadata } from "@/lib/pdsTypes";
import type { DecryptedBlob } from "@/lib/preview";
import { resolveHandleToPds } from "@/lib/oauth";
import { getOpakeWorker } from "@/lib/worker";
import { formatRelativeDate, mimeTypeToFileType, formatFileSize } from "@/lib/format";
import { triggerBrowserDownload } from "@/lib/download";
import type { FileItem } from "@/components/cabinet/types";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

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

/** Resolve a handle to a DID + PDS URL + X25519 public key via core. */
export async function resolveRecipient(handle: string): Promise<RecipientInfo> {
  try {
    const worker = getOpakeWorker();
    const resolved = (await worker.resolveIdentity(handle)) as {
      did: string;
      pds_url: string;
      public_key: Uint8Array;
    };
    return {
      did: resolved.did,
      pdsUrl: resolved.pds_url,
      publicKey: resolved.public_key,
    };
  } catch {
    // Core throws if public key not found — convert to RecipientNotReadyError
    // so the ShareDialog can trigger the pending share path.
    const { did } = await resolveHandleToPds(handle);
    throw new RecipientNotReadyError(handle, did);
  }
}

// Grant creation now goes through cabinetShare (worker API).
// Pending share creation needs a core domain method (Opake::create_pending_share) — deferred.

// ---------------------------------------------------------------------------
// Grant listing (outgoing — from own PDS)
// ---------------------------------------------------------------------------

/** List all outgoing grants from the owner's PDS. */
// Grant listing (outgoing) now goes through cabinetListShares (worker API).

// ---------------------------------------------------------------------------
// Grant listing (incoming — from AppView inbox)
// ---------------------------------------------------------------------------

/** Fetch incoming grants from the AppView inbox via core. */
export async function listIncomingGrants(): Promise<InboxGrantItem[]> {
  const worker = getOpakeWorker();
  const grants = (await worker.listInbox()) as {
    uri: string;
    owner_did: string;
    document_uri: string;
    created_at: string;
  }[];
  return grants.map((g) => ({
    uri: g.uri,
    ownerDid: g.owner_did,
    documentUri: g.document_uri,
    createdAt: g.created_at,
  }));
}

// Grant revocation now goes through cabinetRevokeShare (worker API).

// ---------------------------------------------------------------------------
// Incoming grant resolution (fetch record → unwrap → decrypt metadata)
// ---------------------------------------------------------------------------

/** Resolved incoming grant with decrypted document metadata. */
export interface ResolvedIncomingGrant extends InboxGrantItem {
  readonly metadata: DocumentMetadata;
}

/**
 * Resolve an incoming grant: fetch grant + document from the owner's PDS
 * via core, unwrap key, decrypt metadata. No manual crypto.
 */
export async function resolveIncomingGrant(grant: InboxGrantItem): Promise<ResolvedIncomingGrant> {
  const worker = getOpakeWorker();
  const result = await worker.resolveGrantMetadata(grant.uri);

  return {
    ...grant,
    metadata: result.metadata,
  };
}

/** Download a resolved incoming grant's blob to the user's device. */
export async function downloadIncomingGrant(grant: InboxGrantItem): Promise<void> {
  const worker = getOpakeWorker();
  const result = await worker.downloadFromGrant(grant.uri);
  triggerBrowserDownload(result.plaintext, result.filename, "application/octet-stream");
}

/**
 * Create a decrypt function for previewing an incoming shared document.
 * Suitable for passing directly to `<FilePreview decrypt={...} />`.
 */
export function decryptIncomingDocument(
  grant: InboxGrantItem,
  knownMetadata?: DocumentMetadata,
): () => Promise<DecryptedBlob> {
  return async () => {
    const worker = getOpakeWorker();
    const result = await worker.downloadFromGrant(grant.uri);
    return {
      plaintext: result.plaintext,
      metadata: knownMetadata ?? { name: result.filename },
    };
  };
}
