import { createContext, useContext } from "react";
import type { DirectoryTreeSnapshot } from "@/lib/pdsTypes";

const TreeSnapshotContext = createContext<DirectoryTreeSnapshot | null>(null);

export const TreeSnapshotProvider = TreeSnapshotContext.Provider;

export function useTreeSnapshot(): DirectoryTreeSnapshot | null {
  return useContext(TreeSnapshotContext);
}
