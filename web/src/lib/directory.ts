// Directory orchestration — create/delete folders, manage directory entries.

import {
  authenticatedCreateRecord,
  authenticatedDeleteRecord,
  authenticatedGetRecord,
  authenticatedPutRecord,
} from "@/lib/api";
import { uint8ArrayToBase64 } from "@/lib/encoding";
import { rkeyFromUri } from "@/lib/atUri";
import { getCryptoWorker } from "@/lib/worker";
import type { DirectoryRecord } from "@/lib/pdsTypes";
import type { Session } from "@/lib/storageTypes";

// ---------------------------------------------------------------------------
// Shared: add/remove entries from a directory record
// ---------------------------------------------------------------------------

export async function addEntryToDirectory(
  directoryUri: string | null,
  entryUri: string,
  modifiedAt: string,
  pdsUrl: string,
  did: string,
  session: Session,
): Promise<void> {
  const rkey = directoryUri ? rkeyFromUri(directoryUri) : "self";

  const response = await authenticatedGetRecord<DirectoryRecord>(
    { pdsUrl, did, collection: "app.opake.directory", rkey },
    session,
  );

  const updatedRecord: DirectoryRecord = {
    ...response.value,
    entries: [...response.value.entries, entryUri],
    modifiedAt,
  };

  await authenticatedPutRecord(
    { pdsUrl, did, collection: "app.opake.directory", rkey, record: updatedRecord },
    session,
  );
}

export async function removeEntryFromDirectory(
  directoryUri: string | null,
  entryUri: string,
  pdsUrl: string,
  did: string,
  session: Session,
): Promise<void> {
  const rkey = directoryUri ? rkeyFromUri(directoryUri) : "self";

  const response = await authenticatedGetRecord<DirectoryRecord>(
    { pdsUrl, did, collection: "app.opake.directory", rkey },
    session,
  );

  const updatedRecord: DirectoryRecord = {
    ...response.value,
    entries: response.value.entries.filter((uri) => uri !== entryUri),
    modifiedAt: new Date().toISOString(),
  };

  await authenticatedPutRecord(
    { pdsUrl, did, collection: "app.opake.directory", rkey, record: updatedRecord },
    session,
  );
}

// ---------------------------------------------------------------------------
// Create directory
// ---------------------------------------------------------------------------

export async function createDirectory(
  name: string,
  parentDirectoryUri: string | null,
  pdsUrl: string,
  did: string,
  publicKey: Uint8Array,
  session: Session,
): Promise<string> {
  const worker = getCryptoWorker();

  // Generate content key, then encrypt metadata + wrap key + schema version concurrently
  const contentKey = await worker.generateContentKey();
  const [encryptedMeta, wrappedKey, opakeVersion] = await Promise.all([
    worker.encryptDirectoryMetadata(contentKey, { name }),
    worker.wrapKey(contentKey, publicKey, did),
    worker.schemaVersion(),
  ]);
  const now = new Date().toISOString();

  const directoryRecord: DirectoryRecord = {
    opakeVersion,
    encryption: {
      $type: "app.opake.document#directEncryption",
      envelope: {
        algo: "aes-256-gcm",
        nonce: { $bytes: uint8ArrayToBase64(encryptedMeta.nonce) },
        keys: [wrappedKey],
      },
    },
    encryptedMetadata: {
      ciphertext: { $bytes: uint8ArrayToBase64(encryptedMeta.ciphertext) },
      nonce: { $bytes: uint8ArrayToBase64(encryptedMeta.nonce) },
    },
    entries: [],
    createdAt: now,
    modifiedAt: null,
  };

  const { uri: newDirectoryUri } = await authenticatedCreateRecord(
    { pdsUrl, did, collection: "app.opake.directory", record: directoryRecord },
    session,
  );

  await addEntryToDirectory(parentDirectoryUri, newDirectoryUri, now, pdsUrl, did, session);

  return newDirectoryUri;
}

// ---------------------------------------------------------------------------
// Delete directory (recursive)
// ---------------------------------------------------------------------------

interface DescendantEntry {
  readonly uri: string;
  readonly kind: string;
}

export async function deleteDirectory(
  directoryUri: string,
  parentDirectoryUri: string | null,
  descendants: readonly DescendantEntry[],
  pdsUrl: string,
  did: string,
  session: Session,
): Promise<void> {
  const collectionForKind = (kind: string) =>
    kind === "document" ? "app.opake.document" : "app.opake.directory";

  // Delete all descendants in parallel (PDS records are independent)
  await Promise.all(
    descendants.map((d) =>
      authenticatedDeleteRecord(
        { pdsUrl, did, collection: collectionForKind(d.kind), rkey: rkeyFromUri(d.uri) },
        session,
      ),
    ),
  );

  // Delete the directory record itself + remove from parent concurrently
  await Promise.all([
    authenticatedDeleteRecord(
      { pdsUrl, did, collection: "app.opake.directory", rkey: rkeyFromUri(directoryUri) },
      session,
    ),
    removeEntryFromDirectory(parentDirectoryUri, directoryUri, pdsUrl, did, session),
  ]);
}
