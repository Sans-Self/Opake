// Download orchestration — decrypt blob client-side, trigger browser save.

import { decryptDocumentBlob } from "@/lib/preview";
import type { PdsRecord, DocumentRecord } from "@/lib/pdsTypes";
import type { Session } from "@/lib/storageTypes";

export async function downloadDocument(
  record: PdsRecord<DocumentRecord>,
  pdsUrl: string,
  did: string,
  privateKey: Uint8Array,
  session: Session,
): Promise<void> {
  const { plaintext, metadata } = await decryptDocumentBlob(
    record,
    pdsUrl,
    did,
    privateKey,
    session,
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
