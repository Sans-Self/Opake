// Document metadata decryption — unwrap content key, decrypt envelope,
// update store items via the set callback.

import { getCryptoWorker } from "@/lib/worker";
import { base64ToUint8Array } from "@/lib/encoding";
import { mimeTypeToFileType, formatFileSize } from "@/lib/format";
import type { FileItem } from "@/components/cabinet/types";
import type {
  PdsRecord,
  DocumentRecord,
  DocumentMetadata,
  EncryptedMetadataEnvelope,
  Encryption,
} from "@/lib/pdsTypes";

// Minimal draft shape — avoids coupling to the full DocumentsState type.
interface ItemsDraft {
  items: Record<string, FileItem>;
}

export type SetFn = (fn: (draft: ItemsDraft) => void) => void;

function decryptEnvelope(envelope: EncryptedMetadataEnvelope): {
  readonly ciphertext: Uint8Array;
  readonly nonce: Uint8Array;
} {
  return {
    ciphertext: base64ToUint8Array(envelope.ciphertext.$bytes),
    nonce: base64ToUint8Array(envelope.nonce.$bytes),
  };
}

async function unwrapDirectContentKey(
  encryption: Encryption & { readonly $type: "app.opake.document#directEncryption" },
  did: string,
  privateKey: Uint8Array,
): Promise<Uint8Array> {
  const worker = getCryptoWorker();
  const ourWrappedKey = encryption.envelope.keys.find((wk) => wk.did === did);
  if (!ourWrappedKey) throw new Error("No wrapped key for our DID");
  return worker.unwrapKey(ourWrappedKey, privateKey);
}

export async function decryptDocumentRecord(
  record: PdsRecord<DocumentRecord>,
  did: string,
  privateKey: Uint8Array,
  set: SetFn,
): Promise<void> {
  const encryption = record.value.encryption;
  if (encryption.$type !== "app.opake.document#directEncryption") {
    set((draft) => {
      draft.items[record.uri] = {
        ...draft.items[record.uri],
        name: "[Keyring encrypted]",
        decrypted: true,
      };
    });
    return;
  }

  const contentKey = await unwrapDirectContentKey(encryption, did, privateKey);
  const { ciphertext, nonce } = decryptEnvelope(record.value.encryptedMetadata);
  const worker = getCryptoWorker();
  const metadata: DocumentMetadata = await worker.decryptMetadata(contentKey, ciphertext, nonce);

  set((draft) => {
    draft.items[record.uri] = {
      ...draft.items[record.uri],
      name: metadata.name,
      fileType: metadata.mimeType ? mimeTypeToFileType(metadata.mimeType) : undefined,
      size: metadata.size != null ? formatFileSize(metadata.size) : undefined,
      mimeType: metadata.mimeType ?? undefined,
      tags: metadata.tags ?? [],
      description: metadata.description ?? undefined,
      decrypted: true,
    };
  });
}

export function markDecryptionFailed(uri: string, set: SetFn): void {
  set((draft) => {
    draft.items[uri] = { ...draft.items[uri], name: "[Unable to decrypt]", decrypted: true };
  });
}
