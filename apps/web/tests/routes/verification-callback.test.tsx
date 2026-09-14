// @vitest-environment happy-dom
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VerificationCallbackPage } from "../../src/routes/devices/verification-callback.lazy";

class FakeBroadcastChannel {
  static instances: FakeBroadcastChannel[] = [];
  onmessage: ((event: MessageEvent) => void) | null = null;
  posted: unknown[] = [];
  closed = false;

  constructor(readonly name: string) { FakeBroadcastChannel.instances.push(this); }
  postMessage(value: unknown) { this.posted.push(value); }
  close() { this.closed = true; }
  emit(value: unknown) { this.onmessage?.({ data: value } as MessageEvent); }
}

describe("verification callback popup", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal("BroadcastChannel", FakeBroadcastChannel);
    vi.stubGlobal("close", vi.fn());
    FakeBroadcastChannel.instances = [];
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.unstubAllGlobals();
    window.history.replaceState({}, "", "/devices/verification-callback");
  });

  it("waits for the opener before authorization is dispatched", () => {
    window.history.replaceState({}, "", "/devices/verification-callback?channel=0f3c9d2e-4b7a-4c1d-9e8f-1a2b3c4d5e6f");
    render(<VerificationCallbackPage />);

    expect(screen.getByRole("heading", { name: "Account verification" })).toBeTruthy();
    expect(screen.getByText("Waiting for the verification request from Opake.")).toBeTruthy();
    expect(FakeBroadcastChannel.instances[0]?.posted).toEqual([{ type: "ready" }]);
  });

  it("forwards a complete callback then closes only after dispatch", async () => {
    window.history.replaceState({}, "", "/devices/verification-callback?channel=0f3c9d2e-4b7a-4c1d-9e8f-1a2b3c4d5e6f&code=code&state=csrf&iss=https%3A%2F%2Fas.test");
    const close = vi.mocked(window.close);
    render(<VerificationCallbackPage />);

    expect(FakeBroadcastChannel.instances[0]?.posted).toEqual([{
      type: "callback", code: "code", state: "csrf", issuer: "https://as.test",
    }]);
    expect(screen.getByText(/Verification details were sent/)).toBeTruthy();
    expect(close).not.toHaveBeenCalled();
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    expect(close).toHaveBeenCalledOnce();
  });

  it("does not open a channel for an unbound callback", () => {
    window.history.replaceState({}, "", "/devices/verification-callback?channel=wrong");
    render(<VerificationCallbackPage />);

    expect(FakeBroadcastChannel.instances).toHaveLength(0);
    expect(screen.getByText(/cannot be linked to an active request/)).toBeTruthy();
  });
});
