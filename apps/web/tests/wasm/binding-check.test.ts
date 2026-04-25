import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { describe, it, expect, beforeAll } from "vitest";
import { initSync, bindingCheck } from "@/wasm/opake-wasm/opake";

describe("wasm binding", () => {
  beforeAll(async () => {
    const wasmPath = resolve(__dirname, "../../src/wasm/opake-wasm/opake_bg.wasm");
    const wasmBytes = await readFile(wasmPath);
    initSync({ module: wasmBytes });
  });

  it("binding_check returns WORKS", () => {
    expect(bindingCheck()).toBe("WORKS");
  });
});
