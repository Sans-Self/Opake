import tseslint from "typescript-eslint";
import { makeBaseConfig } from "../../eslint.config.base.ts";

export default tseslint.config(
  ...makeBaseConfig({ tsconfigRootDir: import.meta.dirname }),

  // ---------------------------------------------------------------------------
  // Overrides: scheduler (timer-handle accumulation + imperative lifecycle)
  //
  // `setInterval` returns an opaque handle we have to track in a list
  // so `stop()` can clear each one. That's fundamentally mutable state
  // — the functional rules don't fit the platform-API shape here.
  // ---------------------------------------------------------------------------
  {
    files: ["src/scheduler.ts"],
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
