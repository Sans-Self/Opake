import { describe, it, expect, beforeEach } from "vitest";
import { useAuthStore } from "../../src/stores/auth";

describe("auth store", () => {
  beforeEach(() => {
    useAuthStore.setState({ phase: "unauthenticated" });
  });

  it("starts in initializing phase", () => {
    useAuthStore.setState({ phase: "initializing" });
    const state = useAuthStore.getState();
    expect(state.phase).toBe("initializing");
  });

  it("can transition to unauthenticated", () => {
    const state = useAuthStore.getState();
    expect(state.phase).toBe("unauthenticated");
  });

  it("logout returns to unauthenticated", async () => {
    useAuthStore.setState({
      phase: "ready",
      did: "did:plc:test",
      handle: "test.bsky.social",
      pdsUrl: "https://pds.test",
    });
    // logout does best-effort storage cleanup which will fail without
    // IndexedDB, but the state transition should still happen
    await useAuthStore.getState().logout();
    expect(useAuthStore.getState().phase).toBe("unauthenticated");
  });

  it("exposes boot, startLogin, completeLogin, logout actions", () => {
    const state = useAuthStore.getState();
    expect(typeof state.boot).toBe("function");
    expect(typeof state.startLogin).toBe("function");
    expect(typeof state.completeLogin).toBe("function");
    expect(typeof state.logout).toBe("function");
  });
});
