// Directory tree utilities — path resolution, parent lookup, ancestor chain.

import type { DirectoryTreeSnapshot } from "@/lib/pdsTypes";
import { rkeyFromUri } from "@/lib/atUri";

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

/**
 * Build a URL path suffix for a directory URI in the form `"abc/def"`.
 * Returns null when the directory is the root or missing from the tree —
 * callers should fall back to the base route (e.g. `/cabinet/files`).
 */
export function directoryPathSuffix(
  snapshot: DirectoryTreeSnapshot,
  directoryUri: string | null,
): string | null {
  if (!directoryUri || directoryUri === snapshot.rootUri) return null;
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
  if (!snapshot.directories[directoryUri]) return null;
  const ancestors = ancestorsOf(snapshot, directoryUri);
  const segments = [...ancestors.map((a) => a.rkey), rkeyFromUri(directoryUri)];
  return segments.join("/");
}

/**
 * Resolve a chain of rkey path segments to a directory URI by walking
 * the tree from the root. Returns null if any segment doesn't match.
 *
 * Example: `["abc", "def"]` → find child of root whose rkey is "abc",
 * then find child of that whose rkey is "def".
 */
export function resolveDirectoryFromSplat(
  snapshot: DirectoryTreeSnapshot,
  rkeys: readonly string[],
): string | null {
  if (rkeys.length === 0 || !snapshot.rootUri) return null;

  return rkeys.reduce<string | null>((currentUri, rkey) => {
    if (!currentUri) return null;
    const dir = snapshot.directories[currentUri];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
    if (!dir) return null;
    const child = dir.entries.find((e) => e.type === "directory" && rkeyFromUri(e.uri) === rkey);
    return child?.uri ?? null;
  }, snapshot.rootUri);
}
