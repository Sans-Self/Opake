// Shared ESLint flat-config base for every TypeScript workspace in the repo.
//
// Each package's `eslint.config.ts` imports `makeBaseConfig` and passes its
// own `tsconfigRootDir`, then layers package-specific overrides on top.
// This keeps rule drift between SDK, daemon, react, and web to a minimum
// — if a rule is worth enforcing in one, it's worth enforcing in all.
//
// The base intentionally includes `eslint-plugin-react-hooks` — it's a
// no-op on files that don't use hooks, and shipping it everywhere means
// a package that later adds a hook gets the rules for free.
//
// Packages that render JSX for end users (apps/web) layer jsx-a11y and
// Tailwind sort rules on top separately — they'd be noisy dead weight on
// the hooks-only @opake/react package.

import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import functional from "eslint-plugin-functional";
import security from "eslint-plugin-security";
import noSecrets from "eslint-plugin-no-secrets";
import sonarjs from "eslint-plugin-sonarjs";
import prettier from "eslint-config-prettier";

export interface BaseConfigOptions {
  /**
   * Absolute path to the package directory that contains `tsconfig.json`.
   * Pass `import.meta.dirname` from the package's `eslint.config.ts`.
   */
  readonly tsconfigRootDir: string;
}

/**
 * Build the shared ESLint flat-config array. Callers spread the result
 * and append package-specific overrides (route globs, test globs, etc.).
 */
export function makeBaseConfig(options: BaseConfigOptions) {
  return tseslint.config(
    // -------------------------------------------------------------------------
    // Global ignores (per-package configs can add more)
    // -------------------------------------------------------------------------
    {
      ignores: ["dist/**", "node_modules/**", "**/*.d.ts"],
    },

    // -------------------------------------------------------------------------
    // TypeScript — strict + type-checked (includes eslint:recommended)
    // -------------------------------------------------------------------------
    ...tseslint.configs.strictTypeChecked,
    ...tseslint.configs.stylisticTypeChecked,
    {
      languageOptions: {
        parserOptions: {
          projectService: true,
          tsconfigRootDir: options.tsconfigRootDir,
        },
      },
    },

    // -------------------------------------------------------------------------
    // React hooks — no-op on non-hook files, catches real bugs on hooks
    // -------------------------------------------------------------------------
    reactHooks.configs.flat.recommended,

    // -------------------------------------------------------------------------
    // Functional — immutability enforcement
    // -------------------------------------------------------------------------
    {
      plugins: { functional: functional },
      rules: {
        "functional/immutable-data": "error",
        "functional/no-let": "error",
        "functional/no-loop-statements": "error",
        "functional/prefer-immutable-types": [
          "error",
          {
            enforcement: "None",
            ignoreInferredTypes: true,
            parameters: { enforcement: "None" },
            returnTypes: { enforcement: "None" },
            variables: { enforcement: "ReadonlyShallow" },
          },
        ],
      },
    },

    // -------------------------------------------------------------------------
    // Security
    // -------------------------------------------------------------------------
    security.configs.recommended,
    sonarjs.configs.recommended,
    {
      plugins: { "no-secrets": noSecrets },
      rules: {
        "no-secrets/no-secrets": ["error", { tolerance: 5.5 }],
      },
    },

    // -------------------------------------------------------------------------
    // Prettier — MUST be last (disables conflicting format rules)
    // -------------------------------------------------------------------------
    prettier,

    // -------------------------------------------------------------------------
    // Shared rule tuning
    // -------------------------------------------------------------------------
    {
      rules: {
        // Fire-and-forget promises with `void store.boot()` are idiomatic
        "@typescript-eslint/no-confusing-void-expression": "off",
        // Console is acceptable — we ship to dev tooling, not production-silent
        "no-console": "off",
        // Numbers in template literals are safe and common
        "@typescript-eslint/restrict-template-expressions": [
          "error",
          { allowNumber: true },
        ],
        // Optional chains are safer than non-null assertions
        "@typescript-eslint/non-nullable-type-assertion-style": "off",
        // Persistent false positive with zod `.then(schema.parse)` and
        // similar fp-style method references. The SDK uses this pattern
        // everywhere and it works because zod binds `parse` to the schema.
        "@typescript-eslint/unbound-method": "off",
        // `type` vs `interface` is a style preference. The SDK uses
        // `type` throughout for readonly-friendly records — don't fight it.
        "@typescript-eslint/consistent-type-definitions": "off",

        // --- sonarjs rules that overlap with typescript-eslint ---
        "sonarjs/no-unused-vars": "off",
        "sonarjs/no-dead-store": "off",
        "sonarjs/deprecation": "off",
        // TODOs are intentional markers referencing issue numbers
        "sonarjs/todo-tag": "off",

        // High false-positive rate with bracket notation on typed objects
        "security/detect-object-injection": "off",

        // --- sonarjs rules that don't suit React / TS codebases ---
        "sonarjs/no-nested-template-literals": "off",
        "sonarjs/no-nested-functions": "off",
        "sonarjs/no-nested-conditional": "off",

        // Allow local object construction patterns and browser-API mutation
        "functional/immutable-data": [
          "error",
          {
            ignoreClasses: true,
            ignoreImmediateMutation: true,
            ignoreNonConstDeclarations: true,
            ignoreAccessorPattern: ["window.**", "document.**", "**.current"],
          },
        ],
      },
    },
  );
}
