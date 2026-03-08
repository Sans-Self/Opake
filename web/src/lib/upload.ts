// Upload orchestration — encrypt client-side, upload blob, create record, add to directory.

import {
  authenticatedBlobUpload,
  authenticatedCreateRecord,
  authenticatedXrpc,
  authenticatedPutRecord,
} from "@/lib/api";
import { uint8ArrayToBase64 } from "@/lib/encoding";
import { rkeyFromUri } from "@/lib/atUri";
import { getCryptoWorker } from "@/lib/worker";
import type { DocumentRecord, DirectoryRecord, PdsRecord } from "@/lib/pdsTypes";
import type { Session } from "@/lib/storageTypes";

export async function uploadDocument(
  file: File,
  directoryUri: string | null,
  pdsUrl: string,
  did: string,
  publicKey: Uint8Array,
  session: Session,
): Promise<string> {
  const worker = getCryptoWorker();
  const plaintext = new Uint8Array(await file.arrayBuffer());

  // 1. Generate content key + encrypt blob
  const contentKey = await worker.generateContentKey();
  const blobPayload = await worker.encryptBlob(contentKey, plaintext);

  // 2. Upload blob + wrap key + encrypt metadata — all independent, run concurrently
  const mimeType = file.type || "application/octet-stream";
  const [blobRef, wrappedKey, encryptedMeta] = await Promise.all([
    authenticatedBlobUpload({ pdsUrl, data: blobPayload.ciphertext }, session),
    worker.wrapKey(contentKey, publicKey, did),
    worker.encryptMetadata(contentKey, {
      name: file.name,
      mimeType,
      size: plaintext.byteLength,
    }),
  ]);

  // 5. Build document record
  const opakeVersion = await worker.schemaVersion();
  const now = new Date().toISOString();

  const documentRecord: DocumentRecord = {
    opakeVersion,
    blob: blobRef,
    encryption: {
      $type: "app.opake.document#directEncryption",
      envelope: {
        algo: "aes-256-gcm",
        nonce: { $bytes: uint8ArrayToBase64(blobPayload.nonce) },
        keys: [wrappedKey],
      },
    },
    encryptedMetadata: {
      ciphertext: { $bytes: uint8ArrayToBase64(encryptedMeta.ciphertext) },
      nonce: { $bytes: uint8ArrayToBase64(encryptedMeta.nonce) },
    },
    visibility: "private",
    createdAt: now,
    modifiedAt: null,
  };

  // 6. Create document record on PDS
  const { uri: documentUri } = await authenticatedCreateRecord(
    { pdsUrl, did, collection: "app.opake.document", record: documentRecord },
    session,
  );

  // 7. Add entry to parent directory
  await addEntryToDirectory(directoryUri, documentUri, now, pdsUrl, did, session);

  return documentUri;
}

async function addEntryToDirectory(
  directoryUri: string | null,
  entryUri: string,
  modifiedAt: string,
  pdsUrl: string,
  did: string,
  session: Session,
): Promise<void> {
  const rkey = directoryUri ? rkeyFromUri(directoryUri) : "self";

  // Fetch current directory record
  const response = (await authenticatedXrpc(
    {
      pdsUrl,
      lexicon: `com.atproto.repo.getRecord?repo=${encodeURIComponent(did)}&collection=app.opake.directory&rkey=${encodeURIComponent(rkey)}`,
    },
    session,
  )) as PdsRecord<DirectoryRecord>;

  // Append new entry
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
