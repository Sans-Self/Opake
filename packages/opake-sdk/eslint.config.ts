import tseslint from "typescript-eslint";
import { makeBaseConfig } from "../../eslint.config.base.ts";

export default tseslint.config(
  ...makeBaseConfig({ tsconfigRootDir: import.meta.dirname }),

  // ---------------------------------------------------------------------------
  // Additional ignores — SDK pins a generated WASM bundle
  // ---------------------------------------------------------------------------
  {
    ignores: ["wasm/**"],
  },

  // ---------------------------------------------------------------------------
  // Overrides: Storage adapters
  //
  // The Storage trait requires `Promise`-returning methods, but the
  // MemoryStorage adapter resolves synchronously and the IndexedDB
  // adapter resolves synchronously in most Dexie paths. Silencing
  // `require-await` here matches what the adapters have to look like.
  // Loops + mutation are needed for the bulk put/collection helpers.
  // ---------------------------------------------------------------------------
  {
    files: ["src/storage/**/*.ts"],
    rules: {
      "@typescript-eslint/require-await": "off",
      "functional/no-let": "off",
      "functional/no-loop-statements": "off",
      "functional/immutable-data": "off",
      "functional/prefer-immutable-types": "off",
    },
  },

  // ---------------------------------------------------------------------------
  // Overrides: WASM module binding
  //
  // Lazy-init caches the decoded WASM module on first import. That's a
  // module-level `let` — there's no reactive alternative at this layer.
  // ---------------------------------------------------------------------------
  {
    files: ["src/wasm.ts"],
    rules: {
      "functional/no-let": "off",
      "functional/prefer-immutable-types": "off",
    },
  },

  // ---------------------------------------------------------------------------
  // Overrides: Opake + errors (TC39 decorator target erasure)
  //
  // `withTokenGuard` and `wrapWasmErrors` are TC39 decorators. The
  // method-decorator signature types `target` as `any` by spec —
  // there's no narrower type we could use without giving up the
  // decorator pattern entirely. The `_target` / `_context` naming
  // signals intent; we still need to read the underscore-prefixed
  // params via `.call` to forward the call.
  //
  // Opake holds mutable private state (`ctx`, `refreshPromise`) as
  // part of its lifecycle — destroy() nulls ctx, the refresh
  // single-flight gate flips. Class fields can't be readonly here.
  // ---------------------------------------------------------------------------
  {
    files: ["src/opake.ts", "src/errors.ts"],
    rules: {
      "@typescript-eslint/no-explicit-any": "off",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      "@typescript-eslint/no-unsafe-call": "off",
      "@typescript-eslint/no-unsafe-member-access": "off",
      "@typescript-eslint/no-unsafe-assignment": "off",
      "functional/no-let": "off",
      "functional/prefer-immutable-types": "off",
    },
  },

  // ---------------------------------------------------------------------------
  // Overrides: file-manager + pairing (byte manipulation + iteration)
  //
  // FileManager iterates result arrays and builds decrypted records
  // with imperative loops — the functional patterns don't help here.
  // Pairing has base64 decoding via a classic `for` loop.
  // ---------------------------------------------------------------------------
  {
    files: ["src/file-manager.ts", "src/pairing.ts"],
    rules: {
      "functional/no-let": "off",
      "functional/no-loop-statements": "off",
      "functional/immutable-data": "off",
      "functional/prefer-immutable-types": "off",
    },
  },

  // ---------------------------------------------------------------------------
  // Overrides: test files
  // ---------------------------------------------------------------------------
  {
    files: ["src/__tests__/**/*.ts", "src/**/*.test.ts"],
    rules: {
      "functional/no-let": "off",
      "functional/immutable-data": "off",
      "@typescript-eslint/no-unsafe-assignment": "off",
      "@typescript-eslint/no-unsafe-member-access": "off",
      "@typescript-eslint/no-unsafe-call": "off",
      "@typescript-eslint/no-non-null-assertion": "off",
    },
  },
);
