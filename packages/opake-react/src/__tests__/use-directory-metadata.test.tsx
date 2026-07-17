import { act, render, waitFor } from "@testing-library/react";
import { type ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { QueryClient } from "@tanstack/react-query";
import type { DocumentMetadata, DocumentMetadataResolution } from "@opake/sdk";
import { OpakeProvider } from "../provider";
import {
  useDirectoryMetadata,
  type DirectoryMetadataResult,
} from "../hooks/use-directory-metadata";
import { asOpake, createMockFileManager, createMockOpake, type MockOpake } from "./mock-opake";

const ROOT_URI = "at://did:plc:test/at.opake.directory/self";
const DOC = "at://did:plc:test/at.opake.document/doc1";

// A QueryClient that keeps the assertions deterministic: no built-in retry
// (the hook owns retrying), immediate staleness, and no gc timer racing the
// fake clock.
function testClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 0, gcTime: Infinity },
    },
  });
}

function wrap(opake: MockOpake, children: ReactNode): ReactNode {
  return (
    <OpakeProvider opake={asOpake(opake)} queryClient={testClient()} disableSseAutoStart>
      {children}
    </OpakeProvider>
  );
}

function Probe({
  dirUri,
  uris,
  capture,
}: {
  dirUri: string | null;
  uris: readonly string[];
  capture: { current: DirectoryMetadataResult | null };
}) {
  capture.current = useDirectoryMetadata(null, dirUri, uris);
  return null;
}

function resolvedMetadata(name: string): DocumentMetadata {
  return {
    name,
    mimeType: "text/plain",
    size: 3,
    tags: [],
    description: null,
    createdAt: "2026-03-01T00:00:00Z",
    modifiedAt: null,
  };
}

function resolved(name: string): Record<string, DocumentMetadataResolution> {
  return { [DOC]: { status: "resolved", metadata: resolvedMetadata(name) } };
}

const retryable: Record<string, DocumentMetadataResolution> = {
  [DOC]: { status: "retryable" },
};

const undecryptable: Record<string, DocumentMetadataResolution> = {
  [DOC]: { status: "undecryptable" },
};

/** Install a scripted FileManager so both the watcher and the resolve share it. */
function scriptedOpake(
  resolveImpl: () => Promise<Record<string, DocumentMetadataResolution>>,
): { mock: MockOpake; fm: ReturnType<typeof createMockFileManager> } {
  const mock = createMockOpake();
  const fm = createMockFileManager();
  fm.resolveDocumentMetadataFor.mockImplementation(resolveImpl);
  mock.cabinet.mockImplementation(async () => {
    mock.lastCabinetFm = fm;
    return fm;
  });
  return { mock, fm };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("useDirectoryMetadata", () => {
  // The bug: a freshly created row's name resolve landed on a transient miss
  // and the result was cached with no retrigger, stranding the row on a
  // permanent "Decrypting…". The hook must now self-heal — re-resolve the
  // still-unresolved URI until it lands.
  it("bug__cabinet_name_hydration_self_heals_after_transient_miss", async () => {
    // eslint-disable-next-line functional/no-let -- scripted call counter
    let calls = 0;
    const { mock } = scriptedOpake(async () => {
      calls += 1;
      // First resolve: not visible yet. Subsequent resolves: it lands.
      return calls === 1 ? retryable : resolved("secret.txt");
    });

    const capture = { current: null as DirectoryMetadataResult | null };
    render(wrap(mock, <Probe dirUri={ROOT_URI} uris={[DOC]} capture={capture} />));

    // First pass: unresolved but actively retrying — not a dead end.
    await waitFor(() => {
      expect(capture.current?.statuses[DOC]).toBe("resolving");
    });
    expect(capture.current?.data?.[DOC]).toBeUndefined();

    // The self-heal retry converges without any external event.
    await waitFor(
      () => {
        expect(capture.current?.data?.[DOC]?.name).toBe("secret.txt");
      },
      { timeout: 3000 },
    );
    expect(capture.current?.statuses[DOC]).toBe("resolved");
    expect(calls).toBeGreaterThanOrEqual(2);
  });

  it("marks a no-key document undecryptable and does not retry it", async () => {
    // eslint-disable-next-line functional/no-let -- scripted call counter
    let calls = 0;
    const { mock } = scriptedOpake(async () => {
      calls += 1;
      return undecryptable;
    });

    const capture = { current: null as DirectoryMetadataResult | null };
    render(wrap(mock, <Probe dirUri={ROOT_URI} uris={[DOC]} capture={capture} />));

    await waitFor(() => {
      expect(capture.current?.statuses[DOC]).toBe("undecryptable");
    });

    // Give any stray retry a chance to fire — an undecryptable result is
    // definitive, so the resolve must not be re-run.
    await new Promise((r) => setTimeout(r, 600));
    expect(calls).toBe(1);
    expect(capture.current?.data?.[DOC]).toBeUndefined();
  });

  it("parks a persistently-unresolvable name at retryable once the budget is spent", async () => {
    vi.useFakeTimers();
    // eslint-disable-next-line functional/no-let -- scripted call counter
    let calls = 0;
    const { mock } = scriptedOpake(async () => {
      calls += 1;
      return retryable;
    });

    const capture = { current: null as DirectoryMetadataResult | null };
    await act(async () => {
      render(wrap(mock, <Probe dirUri={ROOT_URI} uris={[DOC]} capture={capture} />));
    });

    // Drain the whole bounded backoff schedule.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });

    expect(capture.current?.statuses[DOC]).toBe("retryable");
    // 1 initial + a bounded number of retries — not an unbounded loop.
    expect(calls).toBeGreaterThan(1);
    expect(calls).toBeLessThanOrEqual(7);

    // A manual retry re-opens the budget.
    const before = calls;
    await act(async () => {
      capture.current?.retry();
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(capture.current?.statuses[DOC]).toBe("resolving");
    expect(calls).toBe(before + 1);
  });
});
