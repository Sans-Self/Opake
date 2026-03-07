import tseslint from "typescript-eslint"
import reactHooks from "eslint-plugin-react-hooks"
import jsxA11y from "eslint-plugin-jsx-a11y"
import functional from "eslint-plugin-functional"
import security from "eslint-plugin-security"
import noSecrets from "eslint-plugin-no-secrets"
import sonarjs from "eslint-plugin-sonarjs"
import prettier from "eslint-config-prettier"

export default tseslint.config(
  // ---------------------------------------------------------------------------
  // Global ignores
  // ---------------------------------------------------------------------------
  {
    ignores: [
      "src/routeTree.gen.ts",
      "src/wasm/**",
      "dist/**",
      "node_modules/**",
    ],
  },

  // ---------------------------------------------------------------------------
  // TypeScript — strict + type-checked (includes eslint:recommended overrides)
  // ---------------------------------------------------------------------------
  ...tseslint.configs.strictTypeChecked,
  ...tseslint.configs.stylisticTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },

  // ---------------------------------------------------------------------------
  // React
  // ---------------------------------------------------------------------------
  reactHooks.configs.flat.recommended,
  jsxA11y.flatConfigs.recommended,

  // ---------------------------------------------------------------------------
  // Functional — immutability enforcement
  // ---------------------------------------------------------------------------
  {
    plugins: { functional: functional },
    rules: {
      "functional/immutable-data": "error",
      "functional/no-let": "error",
      "functional/no-loop-statements": "error",
      "functional/prefer-immutable-types": ["error", {
        enforcement: "None",
        ignoreInferredTypes: true,
        parameters: { enforcement: "None" },
        returnTypes: { enforcement: "None" },
        variables: { enforcement: "ReadonlyShallow" },
      }],
    },
  },

  // ---------------------------------------------------------------------------
  // Security
  // ---------------------------------------------------------------------------
  security.configs.recommended,
  sonarjs.configs.recommended,
  {
    plugins: { "no-secrets": noSecrets },
    rules: {
      "no-secrets/no-secrets": "error",
    },
  },

  // ---------------------------------------------------------------------------
  // Prettier — must be LAST (disables conflicting format rules)
  // ---------------------------------------------------------------------------
  prettier,

  // ---------------------------------------------------------------------------
  // Project-wide rule tuning
  // ---------------------------------------------------------------------------
  {
    rules: {
      // Allow void for fire-and-forget promises (e.g. `void store.boot()`)
      "@typescript-eslint/no-confusing-void-expression": "off",
      // Console is acceptable in a client app with dev tooling
      "no-console": "off",
      // Allow numbers in template literals — common and safe
      "@typescript-eslint/restrict-template-expressions": ["error", { allowNumber: true }],
      // Optional chains are safer than non-null assertions
      "@typescript-eslint/non-nullable-type-assertion-style": "off",

      // --- sonarjs dedup: disable rules that overlap with typescript-eslint ---
      "sonarjs/no-unused-vars": "off",
      "sonarjs/no-dead-store": "off",
      "sonarjs/deprecation": "off",
      // TODOs are intentional markers referencing issue numbers
      "sonarjs/todo-tag": "off",

      // High false-positive rate with bracket notation on typed objects
      "security/detect-object-injection": "off",

      // --- sonarjs: disable rules that don't suit React codebases ---
      // Nested template literals are standard in Tailwind className expressions
      "sonarjs/no-nested-template-literals": "off",
      // React components routinely define callbacks/handlers as nested functions
      "sonarjs/no-nested-functions": "off",
      // JSX uses ternaries extensively for conditional rendering
      "sonarjs/no-nested-conditional": "off",

      // Allow local object construction patterns (headers, records) and browser APIs
      "functional/immutable-data": ["error", {
        ignoreClasses: true,
        ignoreImmediateMutation: true,
        ignoreNonConstDeclarations: true,
        ignoreAccessorPattern: ["window.**", "document.**", "**.current"],
      }],
      // Raise entropy threshold to avoid false positives on charsets/alphabets
      "no-secrets/no-secrets": ["error", { tolerance: 5.5 }],
    },
  },

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
      // returns a Response, not an Error. This is the documented API pattern.
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
)
