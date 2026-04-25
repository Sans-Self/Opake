import { act, render } from "@testing-library/react";
import { StrictMode } from "react";
import { describe, expect, it, vi } from "vitest";
import { OpakeProvider, useOpake, useFileManagerCache } from "../provider";
import { asOpake, createMockOpake } from "./mock-opake";

async function flush() {
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
}

function Reader({ onMount }: { onMount: (opake: ReturnType<typeof useOpake>) => void }) {
  const opake = useOpake();
  onMount(opake);
  return null;
}

describe("OpakeProvider", () => {
  it("exposes the Opake instance via useOpake", () => {
    const mock = createMockOpake();
    const spy = vi.fn();

    render(
      <OpakeProvider opake={asOpake(mock)} disableSseAutoStart>
        <Reader onMount={spy} />
      </OpakeProvider>,
    );

    expect(spy).toHaveBeenCalled();
    // Reference equality — the context should pass through the same object
    const firstCall = spy.mock.calls[0];
    expect(firstCall).toBeDefined();
    expect(firstCall![0]).toBe(mock);
  });

  it("auto-starts the SSE consumer on mount", async () => {
    const mock = createMockOpake();

    render(
      <OpakeProvider opake={asOpake(mock)}>
        <div />
      </OpakeProvider>,
    );
    await flush();

    expect(mock.startSseConsumer).toHaveBeenCalledTimes(1);
    // Called with no arguments — WASM resolves the URL from stored config
    expect(mock.startSseConsumer.mock.calls[0]).toEqual([]);
  });

  it("stops the SSE consumer on unmount to wipe TreeKeeper state", async () => {
    const mock = createMockOpake();

    const { unmount } = render(
      <OpakeProvider opake={asOpake(mock)}>
        <div />
      </OpakeProvider>,
    );
    await flush();

    expect(mock.stopSseConsumer).not.toHaveBeenCalled();
    expect(mock.wipeState).not.toHaveBeenCalled();

    unmount();
    await flush();

    // Cleanup should have run stopSseConsumer + wipeState exactly once:
    // stop the stream so no more events land against freshly-uninstalled
    // scopes, then wipe so the WASM keepers zero cached ContentKeys /
    // decrypted metadata before the next Opake instance takes over.
    expect(mock.stopSseConsumer).toHaveBeenCalledTimes(1);
    expect(mock.wipeState).toHaveBeenCalledTimes(1);
  });

  it("does NOT stop the SSE consumer on unmount when auto-start is disabled", async () => {
    const mock = createMockOpake();

    const { unmount } = render(
      <OpakeProvider opake={asOpake(mock)} disableSseAutoStart>
        <div />
      </OpakeProvider>,
    );
    await flush();

    unmount();
    await flush();

    // With auto-start off, the effect skipped entirely and there's no
    // cleanup path — neither stopSseConsumer nor wipeState fires.
    expect(mock.stopSseConsumer).not.toHaveBeenCalled();
    expect(mock.wipeState).not.toHaveBeenCalled();
  });

  it("skips SSE auto-start when disableSseAutoStart is set", async () => {
    const mock = createMockOpake();

    render(
      <OpakeProvider opake={asOpake(mock)} disableSseAutoStart>
        <div />
      </OpakeProvider>,
    );
    await flush();

    expect(mock.startSseConsumer).not.toHaveBeenCalled();
  });

  it("survives StrictMode double-mount without crashing", async () => {
    const mock = createMockOpake();

    render(
      <StrictMode>
        <OpakeProvider opake={asOpake(mock)}>
          <div />
        </OpakeProvider>
      </StrictMode>,
    );
    await flush();

    // StrictMode runs the effect twice. WASM-side idempotency handles
    // the actual double-start; at the JS level we just verify both
    // calls happen (the mock accepts them) and nothing throws.
    expect(mock.startSseConsumer.mock.calls.length).toBeGreaterThanOrEqual(1);
  });

  it("exposes a FileManagerCache via useFileManagerCache", () => {
    const mock = createMockOpake();

    let cache: ReturnType<typeof useFileManagerCache> | null = null;

    function Probe() {
      cache = useFileManagerCache();
      return null;
    }

    render(
      <OpakeProvider opake={asOpake(mock)} disableSseAutoStart>
        <Probe />
      </OpakeProvider>,
    );

    expect(cache).not.toBeNull();
    expect(typeof cache!.acquire).toBe("function");
    expect(typeof cache!.release).toBe("function");
  });

  it("throws if useOpake is called outside an OpakeProvider", () => {
    function BareConsumer() {
      useOpake();
      return null;
    }

    // React logs the error; suppress for cleaner test output
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => render(<BareConsumer />)).toThrow(/useOpake must be used within an OpakeProvider/);
    errorSpy.mockRestore();
  });

  it("disposes the FileManagerCache on unmount", async () => {
    const mock = createMockOpake();

    let cache: ReturnType<typeof useFileManagerCache> | null = null;

    function Probe() {
      cache = useFileManagerCache();
      return null;
    }

    const { unmount } = render(
      <OpakeProvider opake={asOpake(mock)} disableSseAutoStart>
        <Probe />
      </OpakeProvider>,
    );
    await flush();

    // Acquire something so there's state to dispose
    const fm = await cache!.acquire(null);
    expect(fm).toBeDefined();

    unmount();
    await flush();

    // Cache should be wiped; direct check via has()
    expect(cache!.has(null)).toBe(false);
  });
});
