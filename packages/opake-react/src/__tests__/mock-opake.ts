// Hand-rolled mock of the Opake + FileManager surface that `@opake/react`
// touches. We don't spin up real WASM for hook tests — it's too slow
// and the hooks only care about a small handful of methods.
//
// Each mock returns vi.fn()-backed method shims so tests can assert
// call counts, arguments, and throw behaviors.

import { vi, type Mock } from "vitest";
import type { DirectoryTreeSnapshot, DirectoryWatcher, FileManager, Opake } from "@opake/sdk";

/** A minimal Opake shim — only the methods the React hooks call. */
export interface MockOpake {
  cabinet: Mock<() => Promise<MockFileManager>>;
  workspace: Mock<(keyringUri: string) => Promise<MockFileManager>>;
  startSseConsumer: Mock<(indexerUrl?: string) => Promise<void>>;
  stopSseConsumer: Mock<() => void>;
  wipeState: Mock<() => void>;
  /** Inspect the last FileManager handed out (for cabinet). */
  lastCabinetFm: MockFileManager | null;
  /** Inspect last FM per workspace keyring. */
  workspaceFms: Map<string, MockFileManager>;
}

/** A minimal FileManager shim. */
export interface MockFileManager {
  loadTree: Mock<() => Promise<DirectoryTreeSnapshot>>;
  watchDirectory: Mock<
    (
      directoryUri: string,
      handler: (snapshot: DirectoryTreeSnapshot | null) => void,
    ) => MockDirectoryWatcher
  >;
  dispose: Mock<() => void>;
  /** Fire the most recently installed watcher with a new snapshot. */
  emit: (snapshot: DirectoryTreeSnapshot | null) => void;
  /** The active watcher handler, if any. */
  activeHandler: ((s: DirectoryTreeSnapshot | null) => void) | null;
  /** Target URI of the active watcher, if any. */
  activeWatchedUri: string | null;
  /** Was dispose() called at least once? */
  readonly isDisposed: () => boolean;
}

export interface MockDirectoryWatcher {
  close: Mock<() => void>;
  readonly isClosed: () => boolean;
}

/**
 * Build a fresh mock FileManager with the given initial tree.
 *
 * `initialTree` is what `loadTree()` resolves to. Defaults to an empty
 * cabinet with `rootUri = "at://did:plc:test/app.opake.directory/self"`.
 *
 * The returned mock remembers the most recent watcher it handed out
 * via `activeHandler`; tests call `fm.emit(snapshot)` to simulate an
 * SSE-driven update.
 */
export function createMockFileManager(initialTree?: DirectoryTreeSnapshot): MockFileManager {
  const tree: DirectoryTreeSnapshot =
    initialTree ??
    ({
      rootUri: "at://did:plc:test/app.opake.directory/self",
      directories: {
        "at://did:plc:test/app.opake.directory/self": {
          name: "/",
          entries: [],
          parentUri: null,
        },
      },
    } as DirectoryTreeSnapshot);

  let disposed = false;
  const state: MockFileManager = {
    loadTree: vi.fn(async () => tree),
    watchDirectory: vi.fn(),
    dispose: vi.fn(() => {
      disposed = true;
    }),
    activeHandler: null,
    activeWatchedUri: null,
    emit: (snapshot) => {
      if (state.activeHandler) state.activeHandler(snapshot);
    },
    isDisposed: () => disposed,
  };

  // Wire watchDirectory after `state` exists so the mock can store
  // the handler on the same object.
  state.watchDirectory.mockImplementation((directoryUri, handler) => {
    state.activeHandler = handler;
    state.activeWatchedUri = directoryUri;
    let closed = false;
    const watcher: MockDirectoryWatcher = {
      close: vi.fn(() => {
        closed = true;
        if (state.activeHandler === handler) {
          state.activeHandler = null;
          state.activeWatchedUri = null;
        }
      }),
      isClosed: () => closed,
    };
    return watcher;
  });

  return state;
}

/**
 * Build a fresh mock Opake that vends new FileManagers on each call.
 *
 * The returned object is NOT a full `Opake` — it only implements the
 * methods `@opake/react` calls. Tests that need a full instance should
 * cast via `as unknown as Opake` at the provider boundary.
 */
export function createMockOpake(): MockOpake {
  const mock: MockOpake = {
    cabinet: vi.fn(),
    workspace: vi.fn(),
    startSseConsumer: vi.fn(async () => {}),
    stopSseConsumer: vi.fn(),
    wipeState: vi.fn(),
    lastCabinetFm: null,
    workspaceFms: new Map(),
  };

  mock.cabinet.mockImplementation(async () => {
    const fm = createMockFileManager();
    mock.lastCabinetFm = fm;
    return fm;
  });

  mock.workspace.mockImplementation(async (keyringUri: string) => {
    const fm = createMockFileManager();
    mock.workspaceFms.set(keyringUri, fm);
    return fm;
  });

  return mock;
}

/** Cast helper — narrows our mock to the real Opake interface. */
export function asOpake(mock: MockOpake): Opake {
  return mock as unknown as Opake;
}

/** Cast helper — narrows a mock FileManager to the real interface. */
export function asFileManager(mock: MockFileManager): FileManager {
  return mock as unknown as FileManager;
}

/** Cast helper — narrows a mock watcher to the real interface. */
export function asDirectoryWatcher(mock: MockDirectoryWatcher): DirectoryWatcher {
  return mock as unknown as DirectoryWatcher;
}
