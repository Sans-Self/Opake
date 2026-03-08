// Metadata update orchestration — decrypt content key, merge changes, re-encrypt, persist to PDS.

import { authenticatedPutRecord } from "@/lib/api";
import { uint8ArrayToBase64 } from "@/lib/encoding";
import { rkeyFromUri } from "@/lib/atUri";
import { getCryptoWorker } from "@/lib/worker";
import { unwrapDirectContentKey, decryptEnvelope } from "@/stores/documents/decrypt";
import type { PdsRecord, DocumentRecord, DocumentMetadata } from "@/lib/pdsTypes";
import type { Session } from "@/lib/storageTypes";

/** User-editable metadata fields. mimeType and size are preserved from the original. */
export interface MetadataChanges {
  readonly name: string;
  readonly tags?: string[];
  readonly description?: string;
}

/**
 * Update a document's metadata: decrypt existing, merge user changes (preserving
 * mimeType and size from the original), re-encrypt, and putRecord.
 */
export async function updateDocumentMetadata(
  record: PdsRecord<DocumentRecord>,
  changes: MetadataChanges,
  pdsUrl: string,
  did: string,
  privateKey: Uint8Array,
  session: Session,
): Promise<DocumentRecord> {
  const encryption = record.value.encryption;
  if (encryption.$type !== "app.opake.document#directEncryption") {
    throw new Error("Metadata editing for keyring-encrypted documents is not yet supported");
  }

  const worker = getCryptoWorker();
  const contentKey = await unwrapDirectContentKey(encryption, did, privateKey);

  // Decrypt existing metadata to preserve mimeType and size
  const { ciphertext, nonce } = decryptEnvelope(record.value.encryptedMetadata);
  const existing: DocumentMetadata = await worker.decryptMetadata(contentKey, ciphertext, nonce);

  // Merge: user-editable fields from changes, immutable fields from existing
  const merged: DocumentMetadata = {
    name: changes.name,
    mimeType: existing.mimeType,
    size: existing.size,
    tags: changes.tags,
    description: changes.description,
  };

  const encryptedMeta = await worker.encryptMetadata(contentKey, merged);

  const updatedRecord: DocumentRecord = {
    ...record.value,
    encryptedMetadata: {
      ciphertext: { $bytes: uint8ArrayToBase64(encryptedMeta.ciphertext) },
      nonce: { $bytes: uint8ArrayToBase64(encryptedMeta.nonce) },
    },
    modifiedAt: new Date().toISOString(),
  };

  await authenticatedPutRecord(
    {
      pdsUrl,
      did,
      collection: "app.opake.document",
      rkey: rkeyFromUri(record.uri),
      record: updatedRecord,
    },
    session,
  );

  return updatedRecord;
}
