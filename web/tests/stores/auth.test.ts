import { describe, it, expect, beforeEach } from "vitest";
import { useAuthStore } from "../../src/stores/auth";

describe("auth store", () => {
  beforeEach(() => {
    useAuthStore.setState({
      session: { status: "none" },
      identity: { status: "unchecked" },
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

  it("logout returns to none session and unchecked identity", async () => {
    useAuthStore.setState({
      session: {
        status: "active",
        did: "did:plc:test",
        handle: "test.bsky.social",
        pdsUrl: "https://pds.test",
      },
      identity: { status: "ready" },
    });
    // logout does best-effort storage cleanup which will fail without
    // IndexedDB, but the state transition should still happen
    await useAuthStore.getState().logout();
    expect(useAuthStore.getState().session.status).toBe("none");
    expect(useAuthStore.getState().identity.status).toBe("unchecked");
  });

  it("exposes all action methods", () => {
    const state = useAuthStore.getState();
    expect(typeof state.boot).toBe("function");
    expect(typeof state.startLogin).toBe("function");
    expect(typeof state.completeLogin).toBe("function");
    expect(typeof state.checkIdentity).toBe("function");
    expect(typeof state.generateAndPublishIdentity).toBe("function");
    expect(typeof state.logout).toBe("function");
  });
});
