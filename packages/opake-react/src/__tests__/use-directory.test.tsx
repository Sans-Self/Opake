import { act, render } from "@testing-library/react";
import { type ReactNode } from "react";
import { describe, expect, it } from "vitest";
import type { DirectoryTreeSnapshot } from "@opake/sdk";
import { OpakeProvider } from "../provider";
import { useDirectory } from "../hooks/use-directory";
import { asOpake, createMockOpake, type MockOpake } from "./mock-opake";

function wrap(opake: MockOpake, children: ReactNode): ReactNode {
  return (
    <OpakeProvider opake={asOpake(opake)} disableSseAutoStart>
      {children}
    </OpakeProvider>
  );
}

interface Capture {
  snapshot: DirectoryTreeSnapshot | null;
  isReady: boolean;
  error: Error | null;
  resolvedDirectoryUri: string | null;
}

function emptyCapture(): Capture {
  return { snapshot: null, isReady: false, error: null, resolvedDirectoryUri: null };
}

function Probe({
  keyringUri,
  directoryUri,
  capture,
}: {
  keyringUri: string | null;
  directoryUri: string | null;
  capture: { current: Capture };
}) {
  const result = useDirectory(keyringUri, directoryUri);
  capture.current = { ...result };
  return null;
}

async function flush() {
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
}

const ROOT_URI = "at://did:plc:test/at.opake.directory/self";

describe("useDirectory", () => {
  it("loads tree, resolves root, installs watcher, delivers first snapshot", async () => {
    const mock = createMockOpake();
    const capture = { current: emptyCapture() };

    render(wrap(mock, <Probe keyringUri={null} directoryUri={null} capture={capture} />));
    await flush();

    // FileManager acquired + loadTree called
    expect(mock.cabinet).toHaveBeenCalledTimes(1);
    const fm = mock.lastCabinetFm!;
    expect(fm.loadTree).toHaveBeenCalledTimes(1);

    // Watcher installed on the root URI resolved from the tree
    expect(fm.watchDirectory).toHaveBeenCalledTimes(1);
    expect(fm.activeWatchedUri).toBe(ROOT_URI);

    // Fire the watcher with the current state
    act(() => {
      fm.emit({
        rootUri: ROOT_URI,
        directories: {
          [ROOT_URI]: { name: "/", entries: [], parentUri: null },
        },
      } as DirectoryTreeSnapshot);
    });

    expect(capture.current.isReady).toBe(true);
    expect(capture.current.snapshot?.rootUri).toBe(ROOT_URI);
    expect(capture.current.resolvedDirectoryUri).toBe(ROOT_URI);
  });

  it("uses the explicit directoryUri when provided and skips loadTree", async () => {
    const mock = createMockOpake();
    const capture = { current: emptyCapture() };
    const childUri = "at://did:plc:test/at.opake.directory/photos";

    render(wrap(mock, <Probe keyringUri={null} directoryUri={childUri} capture={capture} />));
    await flush();

    const fm = mock.lastCabinetFm!;
    expect(fm.loadTree).not.toHaveBeenCalled();
    expect(fm.activeWatchedUri).toBe(childUri);
    expect(capture.current.resolvedDirectoryUri).toBe(childUri);
  });

  it("updates the snapshot when the watcher fires again", async () => {
    const mock = createMockOpake();
    const capture = { current: emptyCapture() };

    render(wrap(mock, <Probe keyringUri={null} directoryUri={null} capture={capture} />));
    await flush();

    const fm = mock.lastCabinetFm!;
    const first = {
      rootUri: ROOT_URI,
      directories: { [ROOT_URI]: { name: "/", entries: [], parentUri: null } },
    } as DirectoryTreeSnapshot;
    act(() => fm.emit(first));
    expect(capture.current.snapshot).toBe(first);

    const second = {
      rootUri: ROOT_URI,
      directories: {
        [ROOT_URI]: {
          name: "/",
          entries: [{ uri: "at://new/doc", type: "document" as const }],
          parentUri: null,
        },
      },
    } as DirectoryTreeSnapshot;
    act(() => fm.emit(second));
    expect(capture.current.snapshot).toBe(second);
  });

  it("surfaces deletion by passing null through to state", async () => {
    const mock = createMockOpake();
    const capture = { current: emptyCapture() };

    render(wrap(mock, <Probe keyringUri={null} directoryUri={null} capture={capture} />));
    await flush();

    const fm = mock.lastCabinetFm!;
    act(() =>
      fm.emit({
        rootUri: ROOT_URI,
        directories: { [ROOT_URI]: { name: "/", entries: [], parentUri: null } },
      } as DirectoryTreeSnapshot),
    );
    expect(capture.current.isReady).toBe(true);

    act(() => fm.emit(null));
    expect(capture.current.snapshot).toBeNull();
    expect(capture.current.isReady).toBe(false);
  });

  it("closes watcher on unmount", async () => {
    const mock = createMockOpake();
    const capture = { current: emptyCapture() };

    const { unmount } = render(
      wrap(mock, <Probe keyringUri={null} directoryUri={null} capture={capture} />),
    );
    await flush();

    const fm = mock.lastCabinetFm!;
    expect(fm.activeWatchedUri).toBe(ROOT_URI);

    unmount();
    await flush();

    // Watcher was closed → activeHandler cleared
    expect(fm.activeHandler).toBeNull();
    expect(fm.isDisposed()).toBe(true);
  });

  it("handles empty tree (no rootUri) by returning the empty snapshot", async () => {
    const mock = createMockOpake();
    // Override loadTree to return a tree without a root
    mock.cabinet.mockImplementationOnce(async () => {
      const { createMockFileManager } = await import("./mock-opake");
      const fm = createMockFileManager({
        rootUri: null,
        directories: {},
      } as unknown as DirectoryTreeSnapshot);
      mock.lastCabinetFm = fm;
      return fm;
    });

    const capture = { current: emptyCapture() };
    render(wrap(mock, <Probe keyringUri={null} directoryUri={null} capture={capture} />));
    await flush();

    const fm = mock.lastCabinetFm!;
    expect(fm.watchDirectory).not.toHaveBeenCalled();
    expect(capture.current.snapshot).not.toBeNull();
    expect(capture.current.snapshot?.rootUri).toBeNull();
    expect(capture.current.resolvedDirectoryUri).toBeNull();
  });

  it("switches watcher when directoryUri prop changes", async () => {
    const mock = createMockOpake();
    const capture = { current: emptyCapture() };
    const uriA = "at://did:plc:test/at.opake.directory/a";
    const uriB = "at://did:plc:test/at.opake.directory/b";

    const { rerender } = render(
      wrap(mock, <Probe keyringUri={null} directoryUri={uriA} capture={capture} />),
    );
    await flush();

    const fm = mock.lastCabinetFm!;
    expect(fm.activeWatchedUri).toBe(uriA);

    rerender(wrap(mock, <Probe keyringUri={null} directoryUri={uriB} capture={capture} />));
    await flush();

    expect(fm.activeWatchedUri).toBe(uriB);
    expect(fm.watchDirectory).toHaveBeenCalledTimes(2);
  });
});
