import { act, render } from "@testing-library/react";
import { StrictMode, type ReactNode } from "react";
import { describe, expect, it } from "vitest";
import { OpakeProvider } from "../provider";
import { useFileManager } from "../hooks/use-file-manager";
import { asOpake, createMockOpake, type MockOpake } from "./mock-opake";

/** Wrap children in an OpakeProvider with SSE auto-start disabled. */
function wrap(opake: MockOpake, children: ReactNode): ReactNode {
  return (
    <OpakeProvider opake={asOpake(opake)} disableSseAutoStart>
      {children}
    </OpakeProvider>
  );
}

/** A tiny component that captures the latest useFileManager result
 *  via a mutable ref for assertion. */
interface Capture {
  fileManager: ReturnType<typeof useFileManager>["fileManager"];
  isReady: boolean;
  error: Error | null;
}

function Probe({
  keyringUri,
  capture,
}: {
  keyringUri: string | null;
  capture: { current: Capture };
}) {
  const result = useFileManager(keyringUri);
  capture.current = {
    fileManager: result.fileManager,
    isReady: result.isReady,
    error: result.error,
  };
  return null;
}

async function flushMicrotasks() {
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
}

describe("useFileManager", () => {
  it("acquires a cabinet FileManager on mount", async () => {
    const mock = createMockOpake();
    const capture = { current: { fileManager: null, isReady: false, error: null } as Capture };

    render(wrap(mock, <Probe keyringUri={null} capture={capture} />));
    await flushMicrotasks();

    expect(mock.cabinet).toHaveBeenCalledTimes(1);
    expect(capture.current.isReady).toBe(true);
    expect(capture.current.fileManager).not.toBeNull();
  });

  it("releases and disposes on unmount", async () => {
    const mock = createMockOpake();
    const capture = { current: { fileManager: null, isReady: false, error: null } as Capture };

    const { unmount } = render(wrap(mock, <Probe keyringUri={null} capture={capture} />));
    await flushMicrotasks();

    const cabinetFm = mock.lastCabinetFm;
    expect(cabinetFm).not.toBeNull();
    expect(cabinetFm!.isDisposed()).toBe(false);

    unmount();
    await flushMicrotasks();

    expect(cabinetFm!.isDisposed()).toBe(true);
  });

  it("handles StrictMode double-mount with balanced acquire/release", async () => {
    const mock = createMockOpake();
    const capture = { current: { fileManager: null, isReady: false, error: null } as Capture };

    const { unmount } = render(
      <StrictMode>{wrap(mock, <Probe keyringUri={null} capture={capture} />)}</StrictMode>,
    );
    await flushMicrotasks();

    // StrictMode mounts, unmounts, remounts in dev. The cache should
    // keep the FM alive through the intermediate unmount because
    // acquire/release refcounts balance.
    expect(capture.current.isReady).toBe(true);
    const fm = mock.lastCabinetFm!;
    expect(fm.isDisposed()).toBe(false);

    // Real unmount disposes.
    unmount();
    await flushMicrotasks();
    expect(fm.isDisposed()).toBe(true);
  });

  it("releases old and acquires new when keyringUri changes", async () => {
    const mock = createMockOpake();
    const capture = { current: { fileManager: null, isReady: false, error: null } as Capture };

    const { rerender } = render(wrap(mock, <Probe keyringUri={null} capture={capture} />));
    await flushMicrotasks();
    const cabinetFm = mock.lastCabinetFm!;

    rerender(wrap(mock, <Probe keyringUri="at://ws/a" capture={capture} />));
    await flushMicrotasks();

    expect(cabinetFm.isDisposed()).toBe(true);
    expect(mock.workspace).toHaveBeenCalledWith("at://ws/a");
    expect(mock.workspaceFms.get("at://ws/a")).toBeDefined();
    expect(capture.current.fileManager).toBe(mock.workspaceFms.get("at://ws/a"));
  });

  it("surfaces construction errors via the error field", async () => {
    const mock = createMockOpake();
    mock.cabinet.mockRejectedValueOnce(new Error("boom"));
    const capture = { current: { fileManager: null, isReady: false, error: null } as Capture };

    render(wrap(mock, <Probe keyringUri={null} capture={capture} />));
    await flushMicrotasks();

    expect(capture.current.error).toBeInstanceOf(Error);
    expect(capture.current.error?.message).toBe("boom");
    expect(capture.current.isReady).toBe(false);
  });
});
