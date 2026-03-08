// Delete orchestration — remove document record from PDS and update parent directory.

import { authenticatedDeleteRecord } from "@/lib/api";
import { rkeyFromUri } from "@/lib/atUri";
import { removeEntryFromDirectory } from "@/lib/directory";
import type { Session } from "@/lib/storageTypes";

/**
 * Delete a document from the PDS. Two-step: remove the entry from its parent
 * directory's entries array, then delete the document record itself. The blob
 * becomes orphaned and will be garbage-collected by the PDS.
 */
export async function deleteDocument(
  documentUri: string,
  parentDirectoryUri: string | null,
  pdsUrl: string,
  did: string,
  session: Session,
): Promise<void> {
  await Promise.all([
    removeEntryFromDirectory(parentDirectoryUri, documentUri, pdsUrl, did, session),
    authenticatedDeleteRecord(
      { pdsUrl, did, collection: "app.opake.document", rkey: rkeyFromUri(documentUri) },
      session,
    ),
  ]);
}
