// Download orchestration — fetch encrypted blob, decrypt client-side, trigger browser save.

import { authenticatedBlobFetch } from "@/lib/api";
import { base64ToUint8Array } from "@/lib/encoding";
import { getCryptoWorker } from "@/lib/worker";
import { unwrapDirectContentKey, decryptEnvelope } from "@/stores/documents/decrypt";
import type { PdsRecord, DocumentRecord, DocumentMetadata } from "@/lib/pdsTypes";
import type { Session } from "@/lib/storageTypes";

export async function downloadDocument(
  record: PdsRecord<DocumentRecord>,
  pdsUrl: string,
  did: string,
  privateKey: Uint8Array,
  session: Session,
): Promise<void> {
  const { encryption } = record.value;
  if (encryption.$type !== "app.opake.document#directEncryption") {
    throw new Error("Keyring-encrypted downloads are not yet supported");
  }

  const contentKey = await unwrapDirectContentKey(encryption, did, privateKey);

  // Fetch encrypted blob from PDS
  const cid = record.value.blob.ref.$link;
  const encryptedBlob = await authenticatedBlobFetch({ pdsUrl, did, cid }, session);

  // Decrypt the blob
  const worker = getCryptoWorker();
  const blobNonce = base64ToUint8Array(encryption.envelope.nonce.$bytes);
  const plaintext = await worker.decryptBlob(contentKey, new Uint8Array(encryptedBlob), blobNonce);

  // Decrypt metadata for filename
  const { ciphertext: metaCiphertext, nonce: metaNonce } = decryptEnvelope(
    record.value.encryptedMetadata,
  );
  const metadata: DocumentMetadata = await worker.decryptMetadata(
    contentKey,
    metaCiphertext,
    metaNonce,
  );

  const filename = metadata.name;
  const mimeType = metadata.mimeType ?? "application/octet-stream";

  triggerBrowserDownload(plaintext, filename, mimeType);
}

function triggerBrowserDownload(data: Uint8Array, filename: string, mimeType: string): void {
  const buffer = new ArrayBuffer(data.byteLength);
  new Uint8Array(buffer).set(data);
  const blob = new Blob([buffer], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  // eslint-disable-next-line functional/immutable-data -- DOM side effect at system edge
  anchor.href = url;
  // eslint-disable-next-line functional/immutable-data -- DOM side effect at system edge
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}
