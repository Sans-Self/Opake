// Pure FileItem constructors and tag filtering.

import { formatRelativeDate } from "@/lib/format";
import type { FileItem } from "@/components/cabinet/types";
import type { PdsRecord, DocumentRecord, DirectoryRecord } from "@/lib/pdsTypes";

export function directoryItemFromSnapshot(
  uri: string,
  name: string,
  entryCount: number,
  record: PdsRecord<DirectoryRecord>,
): FileItem {
  return {
    id: uri,
    uri,
    name,
    kind: "folder",
    encrypted: true,
    status: "private",
    items: entryCount,
    modified: formatRelativeDate(record.value.modifiedAt ?? record.value.createdAt),
    decrypted: true,
    tags: [],
  };
}

export function documentPlaceholder(record: PdsRecord<DocumentRecord>): FileItem {
  return {
    id: record.uri,
    uri: record.uri,
    name: "",
    kind: "file",
    encrypted: true,
    status: "private",
    modified: formatRelativeDate(record.value.modifiedAt ?? record.value.createdAt),
    decrypted: false,
    tags: [],
  };
}

export function applyTagFilter(
  items: readonly FileItem[],
  activeFilters: readonly string[],
): FileItem[] {
  if (activeFilters.length === 0) return [...items];
  return items.filter(
    (item) => item.kind === "folder" || item.tags.some((tag) => activeFilters.includes(tag)),
  );
}
