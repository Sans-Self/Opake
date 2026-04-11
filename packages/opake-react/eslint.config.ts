import tseslint from "typescript-eslint";
import { makeBaseConfig } from "../../eslint.config.base.ts";

export default tseslint.config(
  ...makeBaseConfig({ tsconfigRootDir: import.meta.dirname }),

  // ---------------------------------------------------------------------------
  // Overrides: FileManagerCache — refcounted lifecycle, inherently mutable
  //
  // The refcount, the Map of in-flight promises, and the for-loop in
  // `disposeAll` are all part of managing shared resource lifetime. A
  // fully-functional version would need a completely different shape
  // (streams or signals) that doesn't fit the rest of the SDK.
  // ---------------------------------------------------------------------------
  {
    files: ["src/file-manager-cache.ts"],
    rules: {
      "functional/no-let": "off",
      "functional/no-loop-statements": "off",
      "functional/immutable-data": "off",
      "functional/prefer-immutable-types": "off",
      // `type CacheKey = string` is a documentation alias, not redundant
      "sonarjs/redundant-type-aliases": "off",
    },
  },

  // ---------------------------------------------------------------------------
  // Overrides: hooks — React effect cleanup is fundamentally imperative
  //
  // `let cancelled = false` + `cancelled = true` in the effect return
  // is the idiomatic pattern for cancelling in-flight async work on
  // unmount. The functional rules don't fit; AbortController is more
  // verbose for no real win.
  // ---------------------------------------------------------------------------
  {
    files: ["src/hooks/**/*.ts", "src/hooks/**/*.tsx"],
    rules: {
      "functional/no-let": "off",
      "functional/immutable-data": "off",
      "functional/prefer-immutable-types": "off",
    },
  },

  // ---------------------------------------------------------------------------
  // Overrides: test files
  //
  // React 19's `react-hooks/refs` and `react-hooks/set-state-in-effect`
  // rules are correct for production code but test capture patterns
  // (module-level `let cache` + Probe components) intentionally violate
  // them. Mock builders use empty `async () => {}` as vi.fn default
  // implementations; `require-await` doesn't make sense there either.
  // ---------------------------------------------------------------------------
  {
    files: ["src/__tests__/**/*.ts", "src/__tests__/**/*.tsx"],
    rules: {
      "functional/no-let": "off",
      "functional/no-loop-statements": "off",
      "functional/immutable-data": "off",
      "functional/prefer-immutable-types": "off",
      "@typescript-eslint/no-unsafe-assignment": "off",
      "@typescript-eslint/no-unsafe-member-access": "off",
      "@typescript-eslint/no-unsafe-call": "off",
      "@typescript-eslint/no-non-null-assertion": "off",
      "@typescript-eslint/require-await": "off",
      "@typescript-eslint/no-empty-function": "off",
      "sonarjs/void-use": "off",
      "react-hooks/refs": "off",
      "react-hooks/set-state-in-effect": "off",
      "react-hooks/immutability": "off",
      "react-hooks/globals": "off",
    },
  },
);
