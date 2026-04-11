import tseslint from "typescript-eslint";
import jsxA11y from "eslint-plugin-jsx-a11y";
import { makeBaseConfig } from "../../eslint.config.base.ts";

export default tseslint.config(
  ...makeBaseConfig({ tsconfigRootDir: import.meta.dirname }),

  // ---------------------------------------------------------------------------
  // Additional ignores — web has generated + embedded content
  // ---------------------------------------------------------------------------
  {
    ignores: [
      "src/routeTree.gen.ts",
      "src/wasm/**",
      "src/content/**/*.mdx",
      ".output/**",
    ],
  },

  // ---------------------------------------------------------------------------
  // jsx-a11y — web-only, we render UI for humans
  // ---------------------------------------------------------------------------
  jsxA11y.flatConfigs.recommended,

  // ---------------------------------------------------------------------------
  // Overrides: networking files (retry/nonce patterns need let + mutation)
  // ---------------------------------------------------------------------------
  {
    files: ["src/lib/api.ts", "src/lib/oauth.ts"],
    rules: {
      "functional/no-let": "off",
      "functional/immutable-data": "off",
    },
  },

  // ---------------------------------------------------------------------------
  // Overrides: encoding utilities (imperative byte manipulation)
  // ---------------------------------------------------------------------------
  {
    files: ["src/lib/encoding.ts"],
    rules: {
      "functional/no-let": "off",
      "functional/no-loop-statements": "off",
      "functional/immutable-data": "off",
    },
  },

  // ---------------------------------------------------------------------------
  // Overrides: route files
  // ---------------------------------------------------------------------------
  {
    files: ["src/routes/**/*.tsx"],
    rules: {
      // TanStack Router requires default exports
      "import/no-default-export": "off",
      // Zustand store methods accessed via useStore(s => s.method) are safe
      "@typescript-eslint/unbound-method": "off",
      // TanStack Router's beforeLoad guards use `throw redirect(...)` which
      // returns a Response, not an Error. Documented API pattern.
      "@typescript-eslint/only-throw-error": "off",
    },
  },

  // ---------------------------------------------------------------------------
  // Overrides: test files
  // ---------------------------------------------------------------------------
  {
    files: ["tests/**/*.ts", "tests/**/*.tsx"],
    rules: {
      "functional/no-let": "off",
      "functional/immutable-data": "off",
      "@typescript-eslint/no-unsafe-assignment": "off",
      "@typescript-eslint/no-unsafe-member-access": "off",
      "@typescript-eslint/no-unsafe-call": "off",
    },
  },
);
