import { defineConfig } from "tsup";

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
});
