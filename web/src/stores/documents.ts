import { create } from "zustand";

interface Document {
  rkey: string;
  name: string;
  mimeType: string;
  size: number;
  createdAt: string;
}

interface DocumentsState {
  documents: Document[];
  loading: boolean;
  fetch: () => Promise<void>;
}

export const useDocumentsStore = create<DocumentsState>((set) => ({
  documents: [],
  loading: false,

  fetch: async () => {
    set({ loading: true });
    // Stub — replaced by real XRPC calls later
    set({ documents: [], loading: false });
  },
}));
