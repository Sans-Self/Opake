// Directory tree utilities — path resolution, parent lookup, ancestor chain.

import type { DirectoryTreeSnapshot } from "@/lib/pdsTypes";
import { rkeyFromUri } from "@/lib/atUri";

/**
 * Result of {@link findDocumentUriByRkey}. Distinguishes the legitimate
 * "no match" case from the data-integrity-anomalous "multiple documents
 * share an rkey" case, which shouldn't be reachable in practice but
 * surfaces a different UX (and gets logged) if it does.
 */
export type FindDocumentResult =
  | { readonly kind: "found"; readonly uri: string }
  | { readonly kind: "not-found" }
  | { readonly kind: "ambiguous"; readonly uris: readonly string[] };

/**
 * Locate a document URI in a snapshot by its rkey.
 *
 * Cabinet documents are always owned by the current user, so
 * `documentUri(did, rkey)` is sufficient in those routes. Workspace
 * documents are authored by whichever member uploaded them — their
 * URIs are anchored at the uploader's DID, not the workspace owner's
 * or the current viewer's. Callers that only know the rkey must scan
 * the tree for the full URI.
 *
 * rkeys are TID-format (microsecond-clock-derived + a random tail), so
 * cross-PDS collisions are astronomically unlikely. The "ambiguous"
 * branch exists to refuse to silently pick a winner if the underlying
 * data ever does collide — and to give the caller a distinct signal
 * separate from "no match found". `console.warn` fires from here so
 * the anomaly is at least observable in the JS console.
 */
export function findDocumentUriByRkey(
  snapshot: DirectoryTreeSnapshot,
  rkey: string,
): FindDocumentResult {
  const uris = [
    ...new Set(
      Object.values(snapshot.directories)
        .flatMap((info) => info.entries)
        .filter((entry) => entry.type === "document" && rkeyFromUri(entry.uri) === rkey)
        .map((entry) => entry.uri),
    ),
  ];
  if (uris.length === 0) return { kind: "not-found" };
  if (uris.length === 1) return { kind: "found", uri: uris[0] };
  console.warn(
    `[opake] findDocumentUriByRkey: rkey "${rkey}" matches ${String(uris.length)} documents in the snapshot — refusing to guess`,
    { rkey, uris },
  );
  return { kind: "ambiguous", uris };
}

/**
 * Find the parent directory of an entry (document or directory) in the tree.
 * Returns null if the entry is in the root or not found.
 */
export function findParentUri(snapshot: DirectoryTreeSnapshot, targetUri: string): string | null {
  const match = Object.entries(snapshot.directories).find(([, info]) =>
    info.entries.some((e) => e.uri === targetUri),
  );
  return match?.[0] ?? null;
}

/**
 * Walk up the parentUri chain from a directory, returning ancestors
 * in top-down order (closest to root first). Excludes the root itself
 * and the given directory.
 */
export function ancestorsOf(
  snapshot: DirectoryTreeSnapshot,
  dirUri: string | null,
): readonly { readonly uri: string; readonly name: string; readonly rkey: string }[] {
  if (!dirUri) return [];

  const collect = (
    current: string | null,
    acc: readonly { readonly uri: string; readonly name: string; readonly rkey: string }[],
  ): readonly { readonly uri: string; readonly name: string; readonly rkey: string }[] => {
    if (!current || current === snapshot.rootUri) return acc;
    const info = snapshot.directories[current];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
    if (!info) return acc;
    return collect(info.parentUri, [
      { uri: current, name: info.name, rkey: rkeyFromUri(current) },
      ...acc,
    ]);
  };

  const startDir = snapshot.directories[dirUri] as
    | (typeof snapshot.directories)[string]
    | undefined;
  return collect(startDir?.parentUri ?? null, []);
}

