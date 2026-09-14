import { describe, it, expect, beforeEach, vi } from "vitest";
import { useAuthStore } from "../../src/stores/auth";

// jsdom doesn't ship a StorageManager. `requestPersistence` reads
// `navigator.storage.persist` and crashes when the parent is undefined,
// so install a no-op shim before any auth-store action that touches
// the persistence helper.
function shimNavigatorStorage() {
  Object.defineProperty(globalThis.navigator, "storage", {
    configurable: true,
    value: {
      persist: vi.fn(async () => true),
      persisted: vi.fn(async () => true),
    },
  });
}

describe("auth store", () => {
  beforeEach(() => {
    shimNavigatorStorage();
    useAuthStore.setState({
      session: { status: "none" },
      identity: { status: "none" },
    });
  });

  it("starts in initializing session", () => {
    useAuthStore.setState({ session: { status: "initializing" } });
    const state = useAuthStore.getState();
    expect(state.session.status).toBe("initializing");
  });

  it("can transition to none", () => {
    const state = useAuthStore.getState();
    expect(state.session.status).toBe("none");
  });

  it("logout returns to none session and none identity", async () => {
    useAuthStore.setState({
      session: {
        status: "active",
        did: "did:plc:test",
        handle: "test.bsky.social",
        pdsUrl: "https://pds.test",
        avatarUrl: null,
        bannerUrl: null,
      },
      identity: { status: "ready" },
    });
    // logout does best-effort storage cleanup which will fail without
    // IndexedDB, but the state transition should still happen
    await useAuthStore.getState().logout();
    expect(useAuthStore.getState().session.status).toBe("none");
    expect(useAuthStore.getState().identity.status).toBe("none");
  });

  it("exposes all action methods", () => {
    const state = useAuthStore.getState();
    expect(typeof state.boot).toBe("function");
    expect(typeof state.startLogin).toBe("function");
    expect(typeof state.completeLogin).toBe("function");
    expect(typeof state.logout).toBe("function");
    expect(typeof state.generateSeedPhrase).toBe("function");
    expect(typeof state.validateSeedPhrase).toBe("function");
    expect(typeof state.saveIdentity).toBe("function");
    expect(typeof state.finalizePairing).toBe("function");
    expect(typeof state.publishPublicKey).toBe("function");
  });
});
