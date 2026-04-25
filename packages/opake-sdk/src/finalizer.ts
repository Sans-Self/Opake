// FinalizationRegistry for WASM handles.
//
// Registers WASM handles for automatic cleanup when their JS owner is GC'd.
// The Opake context holds tokens, DPoP keys, and identity keys in WASM memory.
// Explicit destroy()/dispose() is strongly recommended for security —
// FinalizationRegistry is a safety net, not a substitute for deterministic
// cleanup. It runs *after* GC, not during, so WASM memory (including
// zeroizable secrets) may persist until the next GC sweep.

type WasmHandle = { free(): void };

const registry = new FinalizationRegistry<WasmHandle>((handle) => {
  try {
    handle.free();
  } catch {
    // Handle may have been freed already (e.g., explicit destroy() raced with GC,
    // or WASM module was unloaded). Either way, nothing to clean up.
  }
});

/**
 * Register a WASM handle for automatic cleanup when its JS owner is GC'd.
 *
 * The registry observes `owner` (weak ref) and holds `handle` (strong ref).
 * When `owner` is collected, the registry calls `handle.free()`.
 *
 * Pass `token` to {@link unregisterCleanup} before manually freeing to
 * prevent double-free.
 */
export function registerCleanup(owner: object, handle: WasmHandle, token: object): void {
  registry.register(owner, handle, token);
}

/**
 * Unregister a previously registered handle. Call this before manual free().
 */
export function unregisterCleanup(token: object): void {
  registry.unregister(token);
}
