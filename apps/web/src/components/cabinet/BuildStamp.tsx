// WASM build stamp — reports which build the browser is actually running.
//
// The binary bakes its build time + git hash in at compile time (see
// crates/opake-wasm/build.rs); `wasmBuildInfo()` reads them back. The point
// is staleness detection: a missing export means the browser served a cached
// `opake_bg.wasm` predating the stamp, so the running code is older than the
// last build. The build *time* is the load-bearing signal — git hash only
// moves on commit, but a rebuild-without-commit still bumps the timestamp.

import { useEffect, useState } from "react";
import { wasmBuildInfo } from "@opake/sdk";

export interface WasmBuildState {
  /** Display string: the build stamp, or a diagnostic when unavailable. */
  readonly text: string;
  /**
   * True when the running binary can't be identified — almost always a cached
   * binary the browser served instead of the fresh build.
   */
  readonly stale: boolean;
}

const LOADING: WasmBuildState = { text: "loading build info…", stale: false };

/**
 * Reads the WASM build stamp via `buildInfo()`. Returns a loading placeholder
 * until the (async) WASM call resolves, then the stamp or a staleness/error
 * diagnostic. Safe against unmount — a late resolution won't set state.
 */
export function useWasmBuildInfo(): WasmBuildState {
  const [state, setState] = useState<WasmBuildState>(LOADING);

  useEffect(() => {
    const controller = new AbortController();
    wasmBuildInfo()
      .then((info) => {
        if (controller.signal.aborted) return;
        setState(
          info
            ? { text: `wasm built ${info.builtAt} · ${info.gitHash}`, stale: false }
            : {
                text: "wasm buildInfo() missing — STALE binary (browser is serving a cached build)",
                stale: true,
              },
        );
      })
      .catch((e: unknown) => {
        if (!controller.signal.aborted) {
          setState({
            text: `buildInfo error: ${e instanceof Error ? e.message : String(e)}`,
            stale: true,
          });
        }
      });
    return () => controller.abort();
  }, []);

  return state;
}

/**
 * Compact, copy-friendly build stamp for app chrome (e.g. the panel footer).
 * Faint by default, error-coloured when the binary looks stale. `select-all`
 * makes the whole stamp one-click-copyable for bug reports; `truncate` keeps
 * it from crowding the footer while the full text stays selectable.
 */
export function BuildStamp({ className = "" }: { readonly className?: string }) {
  const { text, stale } = useWasmBuildInfo();
  return (
    <span
      title="WASM build stamp"
      className={`text-caption select-all ${stale ? "text-error" : "text-text-faint/70"} ${className}`}
    >
      {text}
    </span>
  );
}
