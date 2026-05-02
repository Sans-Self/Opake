// Shared types for the file browsing surface. The FileContext tag flows
// from the route component (cabinet vs workspace) down into FileView /
// EditorView so they can pick the right FileManager + mutation hooks.

import type { DirectoryTreeSnapshot, DocumentMetadata } from "@opake/sdk";
import type { FileItem } from "@/components/cabinet/types";
import { mimeTypeToFileType, formatFileSize, formatRelativeDate } from "@/lib/format";

export type FileContext =
  | { readonly kind: "cabinet" }
  | { readonly kind: "workspace"; readonly keyringUri: string };

/** Translate a FileContext into the keyringUri expected by @opake/react hooks. */
export function keyringUriFor(context: FileContext): string | null {
  return context.kind === "workspace" ? context.keyringUri : null;
}

export interface MetadataChanges {
  readonly name: string;
  readonly tags?: readonly string[];
  readonly description?: string;
}

/**
 * Build the display list for a directory. Subdirectory names come from the
 * tree snapshot; document names / sizes / dates come from the decrypted
 * metadata map. Documents without metadata render as "[Encrypted]" placeholders
 * so the tree structure is visible before the metadata round-trip returns.
 *
 * `sharedUris` is the set of document URIs the caller has at least one
 * outgoing grant for. Items in the set render with the "shared" badge
 * instead of "private". Pass an empty set (or omit) when share state
 * isn't available — workspace contexts skip this since their sharing
 * model is keyring-based, not per-doc grants.
 */
export function snapshotToFileItems(
  directoryUri: string,
  snapshot: DirectoryTreeSnapshot,
  metadata: Readonly<Record<string, DocumentMetadata>>,
  sharedUris: ReadonlySet<string> = new Set(),
): readonly FileItem[] {
  const dir = snapshot.directories[directoryUri];
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
  if (!dir) return [];

  const items: readonly FileItem[] = dir.entries.map((entry) => {
    if (entry.type === "directory") {
      const info = snapshot.directories[entry.uri] as
        | (typeof snapshot.directories)[string]
        | undefined;
      const name = info?.name ?? "Unnamed";
      return {
        id: entry.uri,
        uri: entry.uri,
        name,
        kind: "folder" as const,
        encrypted: false,
        status: "private" as const,
        items: info?.entries.length ?? 0,
        modified: "",
        decrypted: true,
        tags: [],
      };
    }

    const status = sharedUris.has(entry.uri) ? ("shared" as const) : ("private" as const);
    const meta = metadata[entry.uri] as DocumentMetadata | undefined;
    if (meta) {
      return {
        id: entry.uri,
        uri: entry.uri,
        name: meta.name,
        kind: "file" as const,
        fileType: mimeTypeToFileType(meta.mimeType),
        mimeType: meta.mimeType,
        encrypted: true,
        status,
        size: formatFileSize(meta.size),
        modified: meta.modifiedAt
          ? formatRelativeDate(meta.modifiedAt)
          : formatRelativeDate(meta.createdAt),
        decrypted: true,
        tags: [...meta.tags],
        description: meta.description ?? undefined,
      };
    }

    // Metadata not loaded yet — show a decrypt-pending placeholder so the
    // directory structure is visible before the metadata round-trip lands.
    return {
      id: entry.uri,
      uri: entry.uri,
      name: "[Encrypted]",
      kind: "file" as const,
      encrypted: true,
      status,
      modified: "",
      decrypted: false,
      tags: [],
    };
  });

  // Folders first, then files, each sorted by name.
  return [...items].sort((a, b) => {
    if (a.kind !== b.kind) return a.kind === "folder" ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}

