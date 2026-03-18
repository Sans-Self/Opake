// Directory tree — stateful WASM handle for in-memory directory structure.

import { DirectoryTreeHandle } from "@/wasm/opake-wasm/opake";
import type { DirectoryTreeSnapshot, PdsRecord, DirectoryRecord } from "@/lib/pdsTypes";
import {
  DirectoryTreeSnapshotSchema,
  DescendantCountSchema,
  DescendantEntrySchema,
} from "@/lib/schemas";
import { z } from "zod";

// eslint-disable-next-line functional/no-let -- stateful WASM handle held across calls
let directoryTree: DirectoryTreeHandle | null = null;

export const treeApi = {
  buildDirectoryTree(
    records: readonly PdsRecord<DirectoryRecord>[],
    did: string,
    privateKey: Uint8Array,
  ): DirectoryTreeSnapshot {
    if (directoryTree) {
      directoryTree.free();
      directoryTree = null;
    }

    const input = records.map((r) => ({ uri: r.uri, value: r.value }));
    directoryTree = new DirectoryTreeHandle(input, did, privateKey);
    return DirectoryTreeSnapshotSchema.parse(directoryTree.snapshot());
  },

  treeRootUri(): string | undefined {
    return directoryTree?.rootUri();
  },

  treeEntriesFor(uri: string): readonly string[] | null {
    const raw: unknown = directoryTree?.entriesFor(uri) ?? null;
    if (!raw) return null;
    return z.array(z.string()).parse(raw);
  },

  treeDirectoryName(uri: string): string | undefined {
    return directoryTree?.directoryName(uri);
  },

  treeIsDirectory(uri: string): boolean {
    return directoryTree?.isDirectory(uri) ?? false;
  },

  treeFindParent(uri: string): string | undefined {
    return directoryTree?.findParent(uri);
  },

  treeCountDescendants(uri: string): { documents: number; directories: number } {
    if (!directoryTree) return { documents: 0, directories: 0 };
    return DescendantCountSchema.parse(directoryTree.countDescendants(uri));
  },

  treeCollectDescendants(uri: string): readonly { uri: string; kind: string }[] {
    if (!directoryTree) return [];
    return z.array(DescendantEntrySchema).parse(directoryTree.collectDescendants(uri));
  },

  destroyDirectoryTree(): void {
    if (directoryTree) {
      directoryTree.free();
      directoryTree = null;
    }
  },
};
