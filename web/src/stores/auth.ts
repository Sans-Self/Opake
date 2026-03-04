import { create } from "zustand";

interface Account {
  did: string;
  handle: string;
}

interface AuthState {
  accounts: Account[];
  currentDid: string | null;
  login: (handle: string, password: string) => Promise<void>;
  logout: () => void;
  setDefault: (did: string) => void;
}

export const useAuthStore = create<AuthState>((set) => ({
  accounts: [],
  currentDid: null,

  login: async (_handle: string, _password: string) => {
    // Stub — replaced by real OAuth/DPoP flow later
    const mockAccount: Account = {
      did: "did:plc:mock123",
      handle: "mock.bsky.social",
    };
    set((state) => ({
      accounts: [...state.accounts, mockAccount],
      currentDid: mockAccount.did,
    }));
  },

  logout: () => {
    set({ accounts: [], currentDid: null });
  },

  setDefault: (did: string) => {
    set({ currentDid: did });
  },
}));
