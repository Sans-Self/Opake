// Delete orchestration — remove document record from PDS and update parent directory.

import {
  authenticatedGetRecord,
  authenticatedPutRecord,
  authenticatedDeleteRecord,
} from "@/lib/api";
import { rkeyFromUri } from "@/lib/atUri";
import type { DirectoryRecord } from "@/lib/pdsTypes";
import type { Session } from "@/lib/storageTypes";

/**
 * Delete a document from the PDS. Two-step: remove the entry from its parent
 * directory's entries array, then delete the document record itself. The blob
 * becomes orphaned and will be garbage-collected by the PDS.
 */
export async function deleteDocument(
  documentUri: string,
  parentDirectoryRkey: string,
  pdsUrl: string,
  did: string,
  session: Session,
): Promise<void> {
  // 1. Fetch current parent directory record
  const parentRecord = await authenticatedGetRecord<DirectoryRecord>(
    { pdsUrl, did, collection: "app.opake.directory", rkey: parentDirectoryRkey },
    session,
  );

  // 2. Remove the document URI from entries + delete the document record concurrently
  const updatedRecord: DirectoryRecord = {
    ...parentRecord.value,
    entries: parentRecord.value.entries.filter((uri) => uri !== documentUri),
    modifiedAt: new Date().toISOString(),
  };

  await Promise.all([
    authenticatedPutRecord(
      {
        pdsUrl,
        did,
        collection: "app.opake.directory",
        rkey: parentDirectoryRkey,
        record: updatedRecord,
      },
      session,
    ),
    authenticatedDeleteRecord(
      { pdsUrl, did, collection: "app.opake.document", rkey: rkeyFromUri(documentUri) },
      session,
    ),
  ]);
}
