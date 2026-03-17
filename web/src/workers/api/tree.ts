// Directory tree — stateful WASM handle for in-memory directory structure.

import { DirectoryTreeHandle } from "@/wasm/opake-wasm/opake";
import type { DirectoryTreeSnapshot, PdsRecord, DirectoryRecord } from "@/lib/pdsTypes";

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
    return directoryTree.snapshot() as DirectoryTreeSnapshot;
  },

  treeRootUri(): string | undefined {
    return directoryTree?.rootUri();
  },

  treeEntriesFor(uri: string): readonly string[] | null {
    return (directoryTree?.entriesFor(uri) as string[] | null) ?? null;
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
    return directoryTree.countDescendants(uri) as { documents: number; directories: number };
  },

  treeCollectDescendants(uri: string): readonly { uri: string; kind: string }[] {
    if (!directoryTree) return [];
    return directoryTree.collectDescendants(uri) as { uri: string; kind: string }[];
  },

  destroyDirectoryTree(): void {
    if (directoryTree) {
      directoryTree.free();
      directoryTree = null;
    }
  },
};
