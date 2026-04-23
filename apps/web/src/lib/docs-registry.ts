import type { IconName } from "@/components/content/icons";

/**
 * Documentation audiences. The folder names on disk match these keys; the
 * human-facing labels live in `CATEGORY_META` so we can rename the card
 * headings without moving files.
 */
export type DocCategory = "use" | "understand" | "build";

export interface DocMeta {
  readonly slug: string;
  readonly category: DocCategory;
  readonly title: string;
  readonly description: string;
  readonly icon: IconName;
}

export interface CategoryMeta {
  readonly key: DocCategory;
  readonly label: string;
  readonly description: string;
}

/**
 * Audience metadata rendered on the docs landing and sidebar. "Under the
 * hood" intentionally breaks the "For X" pattern — the section covers the
 * crypto + protocol model, which anyone with curiosity can read regardless
 * of whether they'd self-identify as a developer.
 */
export const CATEGORY_META: readonly CategoryMeta[] = [
  {
    key: "use",
    label: "For users",
    description: "Store, share, and recover files with Opake.",
  },
  {
    key: "understand",
    label: "Under the hood",
    description: "How Opake protects your data — the crypto, the records, the protocol.",
  },
  {
    key: "build",
    label: "For developers",
    description: "Program against Opake: CLI, SDK, React hooks, lexicons.",
  },
];

/**
 * Single source of truth for documentation section metadata.
 * Used by both public docs routes and cabinet docs routes.
 */
export const DOCS_REGISTRY: readonly DocMeta[] = [
  // -- For users -------------------------------------------------------------
  {
    slug: "getting-started",
    category: "use",
    title: "Getting Started",
    icon: "sparkles",
    description:
      "Set up your cabinet, create your first encrypted file, and explore the interface.",
  },
  {
    slug: "pairing",
    category: "use",
    title: "Multi-Device Magic",
    icon: "pairing",
    description:
      "Securely transfer your identity keypair to new devices using your PDS as a relay.",
  },
  {
    slug: "seed-phrase",
    category: "use",
    title: "Your Seed Phrase",
    icon: "seedling",
    description: "Back up and recover your identity with a 24-word recovery phrase.",
  },
  {
    slug: "sharing",
    category: "use",
    title: "Sharing",
    icon: "share",
    description: "Share files with another person — one-to-one grants and recipient discovery.",
  },
  {
    slug: "workspaces",
    category: "use",
    title: "Workspaces",
    icon: "group",
    description: "Share folders with teams, families, and research groups — no re-encryption.",
  },
  {
    slug: "troubleshooting",
    category: "use",
    title: "Troubleshooting",
    icon: "question",
    description: "Common problems and how to fix them.",
  },

  // -- Under the hood --------------------------------------------------------
  {
    slug: "encryption",
    category: "understand",
    title: "Encryption & Keys",
    icon: "lock",
    description: "How end-to-end encryption works in Opake and how your keys are managed.",
  },
  {
    slug: "at-protocol",
    category: "understand",
    title: "AT Protocol",
    icon: "network",
    description: "The open standard powering Opake — identity, data portability, and federation.",
  },
  {
    slug: "glossary",
    category: "understand",
    title: "Glossary",
    icon: "book",
    description: "A quick-hit reference for the terminology and acronyms we use in Opake.",
  },

  // -- For developers --------------------------------------------------------
  {
    slug: "cli",
    category: "build",
    title: "The CLI Manual",
    icon: "terminal",
    description:
      "Complete command reference for the Opake CLI — identity, files, sharing, and more.",
  },

  // -- Cross-cutting ---------------------------------------------------------
  {
    slug: "faq",
    // FAQ sits outside a category intentionally; `category: "use"` keeps the
    // type simple while rendering code can special-case the slug to pin it
    // to the top level of the sidebar / landing.
    category: "use",
    title: "FAQ",
    icon: "question",
    description:
      "Common questions about privacy, security, and how Opake compares to alternatives.",
  },
];

export function findDoc(slug: string): DocMeta | undefined {
  return DOCS_REGISTRY.find((d) => d.slug === slug);
}

export function docsByCategory(category: DocCategory): readonly DocMeta[] {
  return DOCS_REGISTRY.filter((d) => d.category === category && d.slug !== "faq");
}
