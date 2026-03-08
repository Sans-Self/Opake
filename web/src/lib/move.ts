// Move orchestration — relocate an entry between directories via two PDS operations.

import { removeEntryFromDirectory, addEntryToDirectory } from "@/lib/directory";
import type { Session } from "@/lib/storageTypes";

/**
 * Move an entry (document or directory) from one parent to another.
 *
 * NOT atomic — two separate putRecord calls. If the add fails after the
 * remove succeeded, attempts recovery by re-adding to the source. If
 * recovery also fails, the entry is orphaned (still exists as a record,
 * appears at root level on next tree rebuild).
 */
export async function moveEntry(
  entryUri: string,
  sourceDirectoryUri: string | null,
  targetDirectoryUri: string | null,
  pdsUrl: string,
  did: string,
  session: Session,
): Promise<void> {
  const modifiedAt = new Date().toISOString();

  // Step 1: Remove from source
  await removeEntryFromDirectory(sourceDirectoryUri, entryUri, pdsUrl, did, session);

  // Step 2: Add to target — if this fails, attempt recovery
  try {
    await addEntryToDirectory(targetDirectoryUri, entryUri, modifiedAt, pdsUrl, did, session);
  } catch (addError) {
    console.error("[move] add to target failed, attempting recovery:", addError);
    try {
      await addEntryToDirectory(sourceDirectoryUri, entryUri, modifiedAt, pdsUrl, did, session);
    } catch (recoveryError) {
      console.error("[move] recovery failed — entry may be orphaned:", recoveryError);
    }
    throw addError;
  }
}
