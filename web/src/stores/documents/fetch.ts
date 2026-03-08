// Paginated PDS record fetching via authenticated XRPC.

import { IndexedDbStorage } from "@/lib/indexeddbStorage";
import { authenticatedXrpc } from "@/lib/api";
import type { ListRecordsResponse, PdsRecord } from "@/lib/pdsTypes";
import type { Session } from "@/lib/storageTypes";

export const storage = new IndexedDbStorage();

export async function fetchAllRecords<T>(
  pdsUrl: string,
  did: string,
  collection: string,
  session: Session,
  cursor?: string,
): Promise<readonly PdsRecord<T>[]> {
  const cursorParam = cursor ? `&cursor=${encodeURIComponent(cursor)}` : "";
  const response = (await authenticatedXrpc(
    {
      pdsUrl,
      lexicon: `com.atproto.repo.listRecords?repo=${encodeURIComponent(did)}&collection=${encodeURIComponent(collection)}&limit=100${cursorParam}`,
    },
    session,
  )) as ListRecordsResponse<T>;

  if (!response.cursor || response.records.length === 0) {
    return response.records;
  }

  const rest = await fetchAllRecords<T>(pdsUrl, did, collection, session, response.cursor);
  return [...response.records, ...rest];
}
