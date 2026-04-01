// Shared directory tree utilities used by both the personal cabinet and workspace stores.

import { rkeyFromUri } from "@/lib/atUri";
import type { DirectoryTreeSnapshot } from "@/lib/pdsTypes";

export interface DirectoryAncestor {
  readonly uri: string;
  readonly name: string;
  readonly rkey: string;
}

/** Find the parent directory URI for a given child URI in the tree. */
export function findParentUri(snapshot: DirectoryTreeSnapshot, childUri: string): string | null {
  return (
    Object.entries(snapshot.directories).find(([, entry]) =>
      entry.entries.includes(childUri),
    )?.[0] ?? null
  );
}

/** Build the ancestor chain from a directory URI up to (but excluding) root. */
export function ancestorsOf(
  snapshot: DirectoryTreeSnapshot,
  directoryUri: string | null,
): readonly DirectoryAncestor[] {
  if (!directoryUri) return [];

  const collectAncestors = (
    current: string,
    acc: readonly DirectoryAncestor[],
  ): readonly DirectoryAncestor[] => {
    const parentUri = findParentUri(snapshot, current);
    if (!parentUri) return acc;
    if (parentUri === snapshot.root_uri) return acc;

    const parentEntry = snapshot.directories[parentUri];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
    if (!parentEntry) return acc;

    const ancestor: DirectoryAncestor = {
      uri: parentUri,
      name: parentEntry.name,
      rkey: rkeyFromUri(parentUri),
    };

    return collectAncestors(parentUri, [ancestor, ...acc]);
  };

  return collectAncestors(directoryUri, []);
}
