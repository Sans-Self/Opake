import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { Opake } from "../src/opake";

// Minimal mock for sessionStorage (node doesn't have one)
function createMockSessionStorage(): globalThis.Storage {
  const store = new Map<string, string>();
  return {
    get length() {
      return store.size;
    },
    key(index: number) {
      return [...store.keys()][index] ?? null;
    },
    getItem(key: string) {
      return store.get(key) ?? null;
    },
    setItem(key: string, value: string) {
      store.set(key, value);
    },
    removeItem(key: string) {
      store.delete(key);
    },
    clear() {
      store.clear();
    },
  };
}

const PENDING_STORAGE_KEY = "opake:pendingLogin";
const PENDING_TTL_MS = 10 * 60 * 1000;

const fakePending = {
  state: "random-csrf-state",
  pkceVerifier: "pkce-verifier-string",
  dpopKey: { privateKeyB64: "abc", publicJwk: { kty: "EC", crv: "P-256", x: "x", y: "y" } },
};

describe("PendingLogin lifecycle", () => {
  let mockStorage: globalThis.Storage;

  beforeEach(() => {
    mockStorage = createMockSessionStorage();
    globalThis.sessionStorage = mockStorage;
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("save then load returns the same pending state", () => {
    Opake.savePendingLogin(fakePending as any);
    const loaded = Opake.loadPendingLogin();
    expect(loaded).toEqual(fakePending);
  });

  it("load clears the key — second load returns null", () => {
    Opake.savePendingLogin(fakePending as any);
    Opake.loadPendingLogin();
    expect(Opake.loadPendingLogin()).toBeNull();
  });

  it("load with no saved state returns null", () => {
    expect(Opake.loadPendingLogin()).toBeNull();
  });

  it("returns null when TTL has expired", () => {
    Opake.savePendingLogin(fakePending as any);
    vi.advanceTimersByTime(PENDING_TTL_MS + 1);
    expect(Opake.loadPendingLogin()).toBeNull();
  });

  it("returns the pending state just before TTL expires", () => {
    Opake.savePendingLogin(fakePending as any);
    vi.advanceTimersByTime(PENDING_TTL_MS - 1);
    expect(Opake.loadPendingLogin()).toEqual(fakePending);
  });

  it("returns null for corrupted JSON without throwing", () => {
    mockStorage.setItem(PENDING_STORAGE_KEY, "{{not json at all");
    expect(Opake.loadPendingLogin()).toBeNull();
  });

  it("clears the key even on parse failure", () => {
    mockStorage.setItem(PENDING_STORAGE_KEY, "garbage");
    Opake.loadPendingLogin();
    expect(mockStorage.getItem(PENDING_STORAGE_KEY)).toBeNull();
  });
});
