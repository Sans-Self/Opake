import { describe, expect, it, vi } from "vitest";
import { FileManagerCache } from "../file-manager-cache";
import { asOpake, createMockOpake } from "./mock-opake";

describe("FileManagerCache", () => {
  it("acquires a cabinet FileManager on first call", async () => {
    const mock = createMockOpake();
    const cache = new FileManagerCache(asOpake(mock));

    const fm = await cache.acquire(null);

    expect(fm).toBeDefined();
    expect(mock.cabinet).toHaveBeenCalledTimes(1);
    expect(cache.refcountOf(null)).toBe(1);
  });

  it("reuses the same FileManager on subsequent acquires", async () => {
    const mock = createMockOpake();
    const cache = new FileManagerCache(asOpake(mock));

    const first = await cache.acquire(null);
    const second = await cache.acquire(null);

    expect(first).toBe(second);
    expect(mock.cabinet).toHaveBeenCalledTimes(1);
    expect(cache.refcountOf(null)).toBe(2);
  });

  it("shares one in-flight promise across concurrent acquires", async () => {
    const mock = createMockOpake();
    const cache = new FileManagerCache(asOpake(mock));

    const [a, b, c] = await Promise.all([
      cache.acquire(null),
      cache.acquire(null),
      cache.acquire(null),
    ]);

    expect(a).toBe(b);
    expect(b).toBe(c);
    expect(mock.cabinet).toHaveBeenCalledTimes(1);
    expect(cache.refcountOf(null)).toBe(3);
  });

  it("decrements refcount on release but keeps the entry until it reaches zero", async () => {
    const mock = createMockOpake();
    const cache = new FileManagerCache(asOpake(mock));

    await cache.acquire(null);
    await cache.acquire(null);
    cache.release(null);

    expect(cache.refcountOf(null)).toBe(1);
    expect(cache.has(null)).toBe(true);
    expect(mock.lastCabinetFm!.isDisposed()).toBe(false);
  });

  it("disposes the FileManager when refcount hits zero", async () => {
    const mock = createMockOpake();
    const cache = new FileManagerCache(asOpake(mock));

    await cache.acquire(null);
    cache.release(null);

    expect(cache.refcountOf(null)).toBe(0);
    expect(cache.has(null)).toBe(false);
    expect(mock.lastCabinetFm!.isDisposed()).toBe(true);
  });

  it("treats cabinet and workspace as independent cache keys", async () => {
    const mock = createMockOpake();
    const cache = new FileManagerCache(asOpake(mock));

    await cache.acquire(null);
    await cache.acquire("at://workspace/kr1");

    expect(cache.refcountOf(null)).toBe(1);
    expect(cache.refcountOf("at://workspace/kr1")).toBe(1);
    expect(mock.cabinet).toHaveBeenCalledTimes(1);
    expect(mock.workspace).toHaveBeenCalledWith("at://workspace/kr1");
  });

  it("keeps workspace entries independent of each other", async () => {
    const mock = createMockOpake();
    const cache = new FileManagerCache(asOpake(mock));

    const fmA = await cache.acquire("at://ws/a");
    const fmB = await cache.acquire("at://ws/b");

    expect(fmA).not.toBe(fmB);
    expect(mock.workspace).toHaveBeenCalledTimes(2);
  });

  it("drops the cache entry on construction error so retries work", async () => {
    const mock = createMockOpake();
    mock.cabinet.mockRejectedValueOnce(new Error("boom"));
    const cache = new FileManagerCache(asOpake(mock));

    await expect(cache.acquire(null)).rejects.toThrow("boom");
    // After the error propagates, the entry should be gone so a
    // retry constructs fresh.
    expect(cache.has(null)).toBe(false);

    // Next acquire succeeds (mock's default implementation).
    const fm = await cache.acquire(null);
    expect(fm).toBeDefined();
    expect(mock.cabinet).toHaveBeenCalledTimes(2);
  });

  it("disposes already-resolved FileManagers on disposeAll", async () => {
    const mock = createMockOpake();
    const cache = new FileManagerCache(asOpake(mock));

    await cache.acquire(null);
    await cache.acquire("at://ws/a");
    await cache.acquire("at://ws/b");

    const cabinetFm = mock.lastCabinetFm!;
    const wsFmA = mock.workspaceFms.get("at://ws/a")!;
    const wsFmB = mock.workspaceFms.get("at://ws/b")!;

    cache.disposeAll();

    expect(cabinetFm.isDisposed()).toBe(true);
    expect(wsFmA.isDisposed()).toBe(true);
    expect(wsFmB.isDisposed()).toBe(true);
    expect(cache.has(null)).toBe(false);
    expect(cache.has("at://ws/a")).toBe(false);
  });

  it("release without acquire is a no-op", async () => {
    const mock = createMockOpake();
    const cache = new FileManagerCache(asOpake(mock));

    // Should not throw
    cache.release(null);
    cache.release("at://nonexistent");

    expect(cache.refcountOf(null)).toBe(0);
  });

  it("disposes promise-resolved FileManager even if released mid-flight", async () => {
    // Simulate: acquire starts the promise, release is called before it
    // resolves, then the promise resolves. Expected: cache entry is
    // already gone, so the resolved FM is disposed immediately.
    const mock = createMockOpake();

    let resolveCabinet: ((fm: ReturnType<typeof createMockOpake>["lastCabinetFm"]) => void) | null =
      null;
    mock.cabinet.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveCabinet = (fm) => resolve(fm as NonNullable<typeof fm>);
        }),
    );

    const cache = new FileManagerCache(asOpake(mock));

    const acquirePromise = cache.acquire(null);
    cache.release(null); // release before resolve

    // Now resolve the construction
    const { createMockFileManager } = await import("./mock-opake");
    const fm = createMockFileManager();
    resolveCabinet!(fm);

    await acquirePromise;

    // Wait a microtask for the then-handler to run
    await new Promise((r) => setTimeout(r, 0));

    expect(fm.isDisposed()).toBe(true);
  });
});

// Suppress unused import warning — vi is needed for the eager-release test
void vi;
