// Regression tests for the WASM handle free-during-borrow bug class.
//
// wasm-bindgen's `async fn(&self)` exports borrow the JS handle for the whole
// future. Calling `.free()` while such a borrow is live panics with
// "attempted to take ownership of Rust value while it was borrowed". Both the
// FileManager and the Opake context now defer `free()` until every in-flight
// operation settles. These tests drive that contract through a fake handle
// that records when (and how often) `free()` is called.

import { describe, it, expect, vi } from "vitest";
import { FileManager } from "../src/file-manager";
import { Opake } from "../src/opake";

interface Deferred<T> {
  readonly promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

// ---------------------------------------------------------------------------
// FileManager
// ---------------------------------------------------------------------------

/** A fake WasmFileManager handle: one controllable `upload`, a `free()` counter. */
function makeFileHandle() {
  let freeCount = 0;
  const op = deferred<{ uri: string }>();
  return {
    op,
    freeCount: () => freeCount,
    upload: vi.fn(() => op.promise),
    free: vi.fn(() => {
      freeCount += 1;
    }),
  };
}

function newFileManager(handle: ReturnType<typeof makeFileHandle>): FileManager {
  return new FileManager(handle as unknown as ConstructorParameters<typeof FileManager>[0]);
}

describe("FileManager handle lifecycle", () => {
  it("bug__dispose_during_inflight_op_defers_free", async () => {
    const handle = makeFileHandle();
    const fm = newFileManager(handle);

    const opPromise = fm.upload(new Uint8Array(), "f.txt", "text/plain");
    // Dispose mid-flight: the handle is still borrowed, so free MUST wait.
    fm.dispose();
    expect(handle.freeCount()).toBe(0);

    handle.op.resolve({ uri: "at://x" });
    await opPromise;

    // The op has settled; free fires exactly once.
    expect(handle.freeCount()).toBe(1);
  });

  it("rejects new calls after dispose (nothing in flight)", async () => {
    const handle = makeFileHandle();
    const fm = newFileManager(handle);

    fm.dispose();

    await expect(fm.upload(new Uint8Array(), "f.txt", "text/plain")).rejects.toThrow(/disposed/i);
  });

  it("rejects new calls issued during a deferred dispose", async () => {
    const handle = makeFileHandle();
    const fm = newFileManager(handle);

    const opPromise = fm.upload(new Uint8Array(), "f.txt", "text/plain");
    fm.dispose(); // deferred — one op still draining

    await expect(fm.upload(new Uint8Array(), "g.txt", "text/plain")).rejects.toThrow(/disposed/i);

    handle.op.resolve({ uri: "at://x" });
    await opPromise;
    expect(handle.freeCount()).toBe(1);
  });

  it("disposes immediately when nothing is in flight", () => {
    const handle = makeFileHandle();
    const fm = newFileManager(handle);

    fm.dispose();

    expect(handle.freeCount()).toBe(1);
  });

  it("bug__double_dispose_frees_once", () => {
    const handle = makeFileHandle();
    const fm = newFileManager(handle);

    fm.dispose();
    fm.dispose();

    expect(handle.freeCount()).toBe(1);
  });

  it("bug__double_dispose_during_inflight_op_frees_once", async () => {
    const handle = makeFileHandle();
    const fm = newFileManager(handle);

    const opPromise = fm.upload(new Uint8Array(), "f.txt", "text/plain");
    fm.dispose();
    fm.dispose();
    expect(handle.freeCount()).toBe(0);

    handle.op.resolve({ uri: "at://x" });
    await opPromise;
    expect(handle.freeCount()).toBe(1);
  });

  it("bug__sync_throwing_op_still_decrements_so_deferred_dispose_fires", async () => {
    // If `op()` throws synchronously, `track()` must still decrement the
    // in-flight count. Otherwise a dispose deferred behind that op would
    // strand `inFlightOps` at 1 and `free()` would never fire.
    let freeCount = 0;
    const handle = {
      upload: vi.fn(() => {
        throw new Error("boom");
      }),
      free: vi.fn(() => {
        freeCount += 1;
      }),
    };
    const fm = newFileManager(handle as unknown as ReturnType<typeof makeFileHandle>);

    // The increment happens synchronously; the rejection's `finally` (which
    // decrements) is scheduled as a microtask. Dispose lands in between, so it
    // must defer — then free once the sync-throw settles.
    const failing = fm.upload(new Uint8Array(), "f.txt", "text/plain");
    fm.dispose();
    expect(freeCount).toBe(0);

    await expect(failing).rejects.toThrow();
    expect(freeCount).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// Opake context
// ---------------------------------------------------------------------------

/** A fake WasmOpakeContext: one controllable `checkSession`, a `free()` counter. */
function makeCtxHandle() {
  let freeCount = 0;
  const op = deferred<void>();
  return {
    op,
    freeCount: () => freeCount,
    checkSession: vi.fn(() => op.promise),
    free: vi.fn(() => {
      freeCount += 1;
    }),
  };
}

// Opake's constructor is private (instances come from Opake.init after WASM
// bootstrap). For a unit test we bypass it with a fake context + storage —
// checkSession touches neither, so no real WASM is needed.
function newOpake(ctx: ReturnType<typeof makeCtxHandle>): Opake {
  const Ctor = Opake as unknown as new (ctx: unknown, storage: unknown, did: string) => Opake;
  return new Ctor(ctx, {}, "did:plc:test");
}

describe("Opake context handle lifecycle", () => {
  it("bug__destroy_during_inflight_op_defers_free", async () => {
    const ctx = makeCtxHandle();
    const opake = newOpake(ctx);

    const opPromise = opake.checkSession();
    opake.destroy();
    expect(ctx.freeCount()).toBe(0);

    ctx.op.resolve();
    await opPromise;
    expect(ctx.freeCount()).toBe(1);
  });

  it("rejects new calls after destroy (nothing in flight)", async () => {
    const ctx = makeCtxHandle();
    const opake = newOpake(ctx);

    opake.destroy();

    await expect(opake.checkSession()).rejects.toThrow(/destroyed/i);
  });

  it("destroys immediately when nothing is in flight", () => {
    const ctx = makeCtxHandle();
    const opake = newOpake(ctx);

    opake.destroy();

    expect(ctx.freeCount()).toBe(1);
  });

  it("bug__double_destroy_frees_once", () => {
    const ctx = makeCtxHandle();
    const opake = newOpake(ctx);

    opake.destroy();
    opake.destroy();

    expect(ctx.freeCount()).toBe(1);
  });

  it("bug__sync_throwing_op_still_decrements_so_deferred_destroy_fires", async () => {
    // Mirror of the FileManager case: a synchronous throw from `op()` must
    // still decrement so a destroy deferred behind it eventually frees.
    let freeCount = 0;
    const ctx = {
      checkSession: vi.fn(() => {
        throw new Error("boom");
      }),
      free: vi.fn(() => {
        freeCount += 1;
      }),
    };
    const opake = newOpake(ctx as unknown as ReturnType<typeof makeCtxHandle>);

    const failing = opake.checkSession();
    opake.destroy();
    expect(freeCount).toBe(0);

    await expect(failing).rejects.toThrow();
    expect(freeCount).toBe(1);
  });
});
