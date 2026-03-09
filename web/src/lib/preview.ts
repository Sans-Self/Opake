// Shared decrypt-without-download logic for file previews and downloads.

import { authenticatedBlobFetch } from "@/lib/api";
import { base64ToUint8Array } from "@/lib/encoding";
import { getCryptoWorker } from "@/lib/worker";
import { unwrapDirectContentKey, decryptEnvelope } from "@/stores/documents/decrypt";
import type { PdsRecord, DocumentRecord, DocumentMetadata } from "@/lib/pdsTypes";
import type { Session } from "@/lib/storageTypes";

export interface DecryptedBlob {
  readonly plaintext: Uint8Array;
  readonly metadata: DocumentMetadata;
}

/**
 * Decrypt a document's blob content. If `knownMetadata` is provided (e.g. from
 * the store), the expensive metadata decryption step is skipped.
 */
export async function decryptDocumentBlob(
  record: PdsRecord<DocumentRecord>,
  pdsUrl: string,
  did: string,
  privateKey: Uint8Array,
  session: Session,
  knownMetadata?: DocumentMetadata,
): Promise<DecryptedBlob> {
  const { encryption } = record.value;
  if (encryption.$type !== "app.opake.document#directEncryption") {
    throw new Error("Keyring-encrypted documents are not yet supported");
  }

  const cid = record.value.blob.ref.$link;

  // Key unwrap and blob fetch are independent — run in parallel
  const [contentKey, encryptedBlob] = await Promise.all([
    unwrapDirectContentKey(encryption, did, privateKey),
    authenticatedBlobFetch({ pdsUrl, did, cid }, session),
  ]);

  const worker = getCryptoWorker();
  const blobNonce = base64ToUint8Array(encryption.envelope.nonce.$bytes);
  const plaintext = await worker.decryptBlob(contentKey, new Uint8Array(encryptedBlob), blobNonce);

  // Skip metadata decryption if caller already has it from the store
  if (knownMetadata) {
    return { plaintext, metadata: knownMetadata };
  }

  const { ciphertext: metaCiphertext, nonce: metaNonce } = decryptEnvelope(
    record.value.encryptedMetadata,
  );
  const metadata: DocumentMetadata = await worker.decryptMetadata(
    contentKey,
    metaCiphertext,
    metaNonce,
  );

  return { plaintext, metadata };
}
