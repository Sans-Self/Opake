// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  getOpake: vi.fn(),
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
}));
vi.mock("../../src/stores/auth", () => ({ getOpake: mocks.getOpake }));
vi.mock("../../src/stores/toast", () => ({ toastError: mocks.toastError, toastSuccess: mocks.toastSuccess }));

const { getOpake, toastError, toastSuccess } = mocks;

import { VerificationSettings } from "../../src/components/cabinet/VerificationSettings";

type Deferred<T> = { promise: Promise<T>; resolve: (value: T) => void; reject: (reason?: unknown) => void };
function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((ok, fail) => { resolve = ok; reject = fail; });
  return { promise, resolve, reject };
}

class FakeBroadcastChannel {
  static instances: FakeBroadcastChannel[] = [];
  onmessage: ((event: MessageEvent) => void) | null = null;
  closed = false;
  posted: unknown[] = [];
  constructor(readonly name: string) { FakeBroadcastChannel.instances.push(this); }
  postMessage(value: unknown) { this.posted.push(value); }
  close() { this.closed = true; }
  emit(value: unknown) { this.onmessage?.({ data: value } as MessageEvent); }
}

function operation(complete = deferred<any>()) {
  return {
    startAuthorization: vi.fn(async () => "https://as.test/authorize"),
    complete: vi.fn(() => complete.promise),
    supplyConfirmation: vi.fn(),
    cancel: vi.fn(),
    stage: "ready",
    completeDeferred: complete,
  };
}

function setup(factory: ReturnType<typeof vi.fn>) {
  getOpake.mockReturnValue({
    bootVerification: { state: "absent" },
    checkOwnVerification: vi.fn(async () => ({ state: "absent" })),
    startVerificationMethodPublication: factory,
    startVerificationMethodRemoval: vi.fn(),
  });
}

async function openAndReady() {
  fireEvent.click(screen.getByRole("button", { name: "Set up verification" }));
  await act(async () => { FakeBroadcastChannel.instances.at(-1)?.emit({ type: "ready" }); });
}

describe("VerificationSettings operation lifetime", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal("BroadcastChannel", FakeBroadcastChannel);
    vi.stubGlobal("open", vi.fn());
    FakeBroadcastChannel.instances = [];
    getOpake.mockReset(); toastError.mockReset(); toastSuccess.mockReset();
  });
  afterEach(() => { cleanup(); vi.useRealTimers(); vi.unstubAllGlobals(); });

  it("reports a blocked pre-opened popup without starting a grant", async () => {
    const factory = vi.fn(); setup(factory);
    render(<VerificationSettings />);
    fireEvent.click(screen.getByRole("button", { name: "Set up verification" }));
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    expect(factory).not.toHaveBeenCalled();
    expect(toastError).toHaveBeenCalledWith("The verification popup was blocked.");
  });

  it("does not resurrect after cancellation before popup readiness", async () => {
    const factory = vi.fn(); setup(factory);
    render(<VerificationSettings />);
    fireEvent.click(screen.getByRole("button", { name: "Set up verification" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel verification" }));
    await act(async () => { FakeBroadcastChannel.instances.at(-1)?.emit({ type: "ready" }); });
    expect(factory).not.toHaveBeenCalled();
  });

  it("cancels a holder that arrives after factory cancellation", async () => {
    const factoryDeferred = deferred<any>();
    const factory = vi.fn(() => factoryDeferred.promise); setup(factory);
    render(<VerificationSettings />);
    await openAndReady();
    fireEvent.click(screen.getByRole("button", { name: "Cancel verification" }));
    const late = operation();
    await act(async () => { factoryDeferred.resolve(late); });
    expect(late.cancel).toHaveBeenCalledOnce();
    expect(late.startAuthorization).not.toHaveBeenCalled();
  });

  it("cancels the live holder during completion and leaves no resumable operation", async () => {
    const live = operation();
    const factory = vi.fn(async () => live); setup(factory);
    const view = render(<VerificationSettings />);
    await openAndReady();
    await act(async () => { FakeBroadcastChannel.instances.at(-1)?.emit({ type: "callback", code: "c", state: "s", issuer: "https://as.test" }); });
    expect(live.complete).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: "Cancel verification" }));
    expect(live.cancel).toHaveBeenCalledOnce();
    // The cancelled completion remains the current attempt until it settles;
    // an old finally therefore cannot clear or overlap a replacement flow.
    fireEvent.click(screen.getByRole("button", { name: "Set up verification" }));
    expect(factory).toHaveBeenCalledOnce();
    view.unmount();
    expect(live.cancel).toHaveBeenCalledOnce();
    await act(async () => {
      live.completeDeferred.resolve({
        mutation: "Unknown",
        cleanup: "Failed",
        reconciliation: "Unavailable",
      });
    });
    expect(toastError).toHaveBeenCalledWith(
      "The submission outcome is unknown; inspect the current verification state before retrying.",
    );
    expect(toastError).toHaveBeenCalledWith("Temporary authorization cleanup encountered a failure.");
  });

  it("offers explicit removal for a substituted method", () => {
    setup(vi.fn());
    getOpake.mockReturnValue({
      ...getOpake(),
      bootVerification: { state: "substitution" },
    });
    render(<VerificationSettings />);
    expect(screen.getByRole("button", { name: "Remove substituted verification method" })).toBeTruthy();
  });

  it("refreshes the mounted view after a cancelled completion settles", async () => {
    const live = operation();
    const checkOwnVerification = vi.fn(async () => ({ state: "substitution" }));
    getOpake.mockReturnValue({
      bootVerification: { state: "absent" },
      checkOwnVerification,
      startVerificationMethodPublication: vi.fn(async () => live),
      startVerificationMethodRemoval: vi.fn(),
    });
    render(<VerificationSettings />);
    await openAndReady();
    await act(async () => { FakeBroadcastChannel.instances.at(-1)?.emit({ type: "callback", code: "c", state: "s", issuer: "https://as.test" }); });
    fireEvent.click(screen.getByRole("button", { name: "Cancel verification" }));
    await act(async () => { live.completeDeferred.resolve({ mutation: "Canceled", cleanup: "Attempted", reconciliation: null }); });
    expect(checkOwnVerification).toHaveBeenCalled();
    expect(screen.getByText(/published verification method uses a key/)).toBeTruthy();
  });

  it("cancels an unattended callback at the finite deadline", async () => {
    const live = operation();
    const factory = vi.fn(async () => live); setup(factory);
    render(<VerificationSettings />);
    await openAndReady();
    await act(async () => { FakeBroadcastChannel.instances.at(-1)?.emit({ type: "callback", code: "c", state: "s", issuer: "https://as.test" }); });
    await act(async () => { await vi.advanceTimersByTimeAsync(10 * 60 * 1000); });
    expect(live.cancel).toHaveBeenCalledOnce();
  });

  it("disposes the live holder when OAuth returns an error before completion", async () => {
    const live = operation();
    const factory = vi.fn(async () => live); setup(factory);
    render(<VerificationSettings />);
    await openAndReady();
    await act(async () => { FakeBroadcastChannel.instances.at(-1)?.emit({ type: "error" }); });
    expect(live.complete).not.toHaveBeenCalled();
    expect(live.cancel).toHaveBeenCalledOnce();
    expect(toastError).toHaveBeenCalledWith("Authorization server refused the verification operation.");
  });

  it("renders structured delivery-unknown refusal with cleanup and reconciliation results", async () => {
    const live = operation();
    const factory = vi.fn(async () => live); setup(factory);
    render(<VerificationSettings />);
    await openAndReady();
    await act(async () => { FakeBroadcastChannel.instances.at(-1)?.emit({ type: "callback", code: "c", state: "s", issuer: "https://as.test" }); });
    await act(async () => {
      live.completeDeferred.resolve({
        mutation: { Refused: { reason: "ConfirmationDeliveryUnknown" } },
        cleanup: "Failed",
        reconciliation: "Unavailable",
      });
    });
    expect(toastError).toHaveBeenCalledWith("Confirmation delivery could not be determined.");
    expect(toastError).toHaveBeenCalledWith("Temporary authorization cleanup encountered a failure.");
    expect(toastError).toHaveBeenCalledWith("A fresh DID read was unavailable, so the submission remains uncertain.");
  });
});
