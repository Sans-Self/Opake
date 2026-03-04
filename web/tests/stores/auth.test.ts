import { describe, it, expect, beforeEach } from "vitest";
import { useAuthStore } from "../../src/stores/auth";

describe("auth store", () => {
  beforeEach(() => {
    useAuthStore.setState({ accounts: [], currentDid: null });
  });

  it("starts with no session", () => {
    const state = useAuthStore.getState();
    expect(state.accounts).toEqual([]);
    expect(state.currentDid).toBeNull();
  });

  it("setDefault updates currentDid", () => {
    useAuthStore.getState().setDefault("did:plc:abc123");
    expect(useAuthStore.getState().currentDid).toBe("did:plc:abc123");
  });

  it("login adds an account and sets current", async () => {
    await useAuthStore.getState().login("test.bsky.social", "password");
    const state = useAuthStore.getState();
    expect(state.accounts).toHaveLength(1);
    expect(state.currentDid).toBe("did:plc:mock123");
  });

  it("logout clears everything", async () => {
    await useAuthStore.getState().login("test.bsky.social", "password");
    useAuthStore.getState().logout();
    const state = useAuthStore.getState();
    expect(state.accounts).toEqual([]);
    expect(state.currentDid).toBeNull();
  });
});
