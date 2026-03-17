// Shared decrypt-without-download logic for file previews and downloads.

import { authenticatedBlobFetch } from "@/lib/api";
import { base64ToUint8Array } from "@/lib/encoding";
import { getOpakeWorker } from "@/lib/worker";
import { unwrapDirectContentKey, decryptEnvelope } from "@/stores/documents/decrypt";
import { useDocumentsStore } from "@/stores/documents/store";
import { useAuthStore } from "@/stores/auth";
import { IndexedDbStorage } from "@/lib/indexeddbStorage";
import type { PdsRecord, DocumentRecord, DocumentMetadata } from "@/lib/pdsTypes";
import type { Session } from "@/lib/storageTypes";

const previewStorage = new IndexedDbStorage();

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

  const worker = getOpakeWorker();
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

/**
 * Create a decrypt function for a document owned by the current user.
 * Pulls the record from the documents store, session + identity from IndexedDB.
 * Suitable for passing directly to `<FilePreview decrypt={...} />`.
 */
export function decryptOwnDocument(documentUri: string): () => Promise<DecryptedBlob> {
  return async () => {
    const state = useDocumentsStore.getState();
    const record = state.documentRecords[documentUri] as PdsRecord<DocumentRecord> | undefined;
    if (!record) throw new Error("Document record not found");

    const authState = useAuthStore.getState();
    if (authState.session.status !== "active") throw new Error("Not authenticated");

    const { did, pdsUrl } = authState.session;
    const session = await previewStorage.loadSession(did);
    const identity = await previewStorage.loadIdentity(did);
    const privateKey = base64ToUint8Array(identity.private_key);

    const storeItem = state.items[documentUri];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
    const knownMetadata: DocumentMetadata | undefined = storeItem?.decrypted
      ? {
          name: storeItem.name,
          mimeType: storeItem.mimeType,
          tags: storeItem.tags,
          description: storeItem.description,
        }
      : undefined;

    return decryptDocumentBlob(record, pdsUrl, did, privateKey, session, knownMetadata);
  };
}
