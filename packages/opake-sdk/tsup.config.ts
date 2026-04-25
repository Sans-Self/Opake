import { defineConfig } from "tsup";
import { copyFileSync } from "node:fs";

export default defineConfig({
  entry: {
    index: "src/index.ts",
    "storage/indexeddb": "src/storage/indexeddb.ts",
  },
  format: ["esm", "cjs"],
  dts: true,
  sourcemap: true,
  clean: true,
  outDir: "dist",
  external: ["dexie"],
  onSuccess: async () => {
    // The WASM glue code resolves opake_bg.wasm relative to itself via
    // `new URL('opake_bg.wasm', import.meta.url)`. Since tsup bundles the
    // glue into dist/, the binary needs to be there too.
    copyFileSync("wasm/opake_bg.wasm", "dist/opake_bg.wasm");
  },
});
