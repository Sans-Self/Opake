// Upload orchestration — encrypt client-side, upload blob, create record, add to directory.

import { authenticatedBlobUpload, authenticatedCreateRecord } from "@/lib/api";
import { uint8ArrayToBase64 } from "@/lib/encoding";
import { getCryptoWorker } from "@/lib/worker";
import { addEntryToDirectory } from "@/lib/directory";
import type { DocumentRecord } from "@/lib/pdsTypes";
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
