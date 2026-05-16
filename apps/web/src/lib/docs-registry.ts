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
  /**
   * Sub-section inside a category. Pages with a group live at
   * `/docs/{group}/{slug}`; pages without a group live at `/docs/{slug}`.
   * Used to book-ify the `build/sdk/*` and `build/react/*` references
   * without turning every flat doc into a nested URL.
   */
  readonly group?: string;
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
 * Human-facing labels for the sub-group keys used inside a category. Pages
 * register with `group: "sdk"`; the sidebar renders it as `@opake/sdk`.
 */
export const GROUP_META: Readonly<Record<string, string>> = {
  sdk: "@opake/sdk",
  react: "@opake/react",
};

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
    key: "build",
    label: "For developers",
    description: "Program against Opake: CLI, SDK, React hooks, lexicons.",
  },
  {
    key: "understand",
    label: "Under the hood",
    description: "How Opake protects your data — the crypto, the records, the protocol.",
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
      "Move your identity onto a new phone or laptop without putting it on the network in plaintext.",
  },
  {
    slug: "seed-phrase",
    category: "use",
    title: "What your key actually looks like",
    icon: "seedling",
    description:
      "Twenty-four words that can bring your identity back on any device — your fallback when nothing else is left.",
  },
  {
    slug: "sharing",
    category: "use",
    title: "Sharing",
    icon: "share",
    description: "Share a file with someone else. All you need is their handle.",
  },
  {
    slug: "workspaces",
    category: "use",
    title: "Workspaces",
    icon: "group",
    description:
      "Share folders with teams, families, and research groups. Add and remove people without re-uploading files.",
  },
  {
    slug: "troubleshooting",
    category: "use",
    title: "Troubleshooting",
    icon: "question",
    description: "Common problems and how to fix them.",
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
  {
    slug: "overview",
    group: "sdk",
    category: "build",
    title: "@opake/sdk — Overview",
    icon: "book",
    description:
      "Install, initialise, and ship your first encrypted upload with the TypeScript SDK.",
  },
  {
    slug: "authentication",
    group: "sdk",
    category: "build",
    title: "Authentication",
    icon: "lock",
    description:
      "OAuth redirect flow, app-password fallback, proactive token refresh, session states.",
  },
  {
    slug: "identity",
    group: "sdk",
    category: "build",
    title: "Identity & pairing",
    icon: "seedling",
    description:
      "Fresh creation, seed-phrase recovery, and device pairing. Private keys never touch JS.",
  },
  {
    slug: "files",
    group: "sdk",
    category: "build",
    title: "Files & directories",
    icon: "folder",
    description:
      "The FileManager contract: upload, download, tree reads, structure changes, metadata and content edits, live subscriptions.",
  },
  {
    slug: "sharing",
    group: "sdk",
    category: "build",
    title: "Sharing",
    icon: "share",
    description:
      "One-to-one grants, pending shares for recipients who haven't set up yet, inbox reads, and revocation semantics.",
  },
  {
    slug: "workspaces",
    group: "sdk",
    category: "build",
    title: "Workspaces",
    icon: "group",
    description:
      "Create, list, and manage shared encrypted folders. Membership roles, key rotation, and the federation chain model.",
  },
  {
    slug: "events",
    group: "sdk",
    category: "build",
    title: "Live updates",
    icon: "lightning",
    description:
      "The SSE consumer lifecycle, bootstrap + reconnect semantics, token exchange, and keeper-backed watchers.",
  },
  {
    slug: "storage",
    group: "sdk",
    category: "build",
    title: "Storage interface",
    icon: "book",
    description:
      "The Storage interface, built-in implementations, and how to write your own backend.",
  },
  {
    slug: "overview",
    group: "react",
    category: "build",
    title: "@opake/react — Overview",
    icon: "book",
    description:
      "Provider, hook anatomy, and the optimistic overlay that keeps the UI in sync during in-flight mutations.",
  },
  {
    slug: "queries",
    group: "react",
    category: "build",
    title: "Reading hooks",
    icon: "folder",
    description:
      "Subscription-backed and react-query-backed read hooks — directory trees, workspaces, inbox, shares, metadata.",
  },
  {
    slug: "mutations",
    group: "react",
    category: "build",
    title: "Writing hooks",
    icon: "share",
    description:
      "Upload, delete, move, directory operations, sharing mutations, and how the optimistic overlay behaves.",
  },
  {
    slug: "live-updates",
    group: "react",
    category: "build",
    title: "Live updates",
    icon: "lightning",
    description:
      "Gating the SSE auto-start, running the daemon, manually invalidating cached queries, account-switch semantics.",
  },
  {
    slug: "lexicons",
    category: "build",
    title: "Lexicon reference",
    icon: "network",
    description:
      "The atproto collections, record schemas, and encryption envelope Opake publishes to a PDS.",
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

/** URL path for a doc — nested under `group` if set, flat otherwise. */
export function docPath(doc: DocMeta): string {
  return doc.group ? `/docs/${doc.group}/${doc.slug}` : `/docs/${doc.slug}`;
}

export interface DocGroupBlock {
  readonly key: string;
  readonly label: string;
  readonly docs: readonly DocMeta[];
}

/**
 * Split a category's docs into (ungrouped, grouped) for sidebar rendering.
 * Flat docs render as a straight list; grouped docs render nested under
 * their group label from {@link GROUP_META}.
 */
/**
 * Next doc in the registry that shares either the current doc's group
 * (nested pages like sdk/*) or its category (flat pages). The registry
 * order is the canonical reading order, so "next" is literally the next
 * entry that matches. `null` when the current doc is the last in its
 * sequence, which the UI should render as nothing.
 */
export function nextDoc(currentSlug: string): DocMeta | null {
  const index = DOCS_REGISTRY.findIndex((d) => d.slug === currentSlug);
  if (index === -1) return null;
  const current = DOCS_REGISTRY[index]!;
  for (let i = index + 1; i < DOCS_REGISTRY.length; i++) {
    const candidate = DOCS_REGISTRY[i]!;
    if (candidate.slug === "faq") continue; // FAQ is cross-cutting, not a chapter
    const sameGroup = current.group !== undefined && candidate.group === current.group;
    const sameFlatCategory =
      current.group === undefined &&
      candidate.group === undefined &&
      candidate.category === current.category;
    if (sameGroup || sameFlatCategory) return candidate;
    // Stop walking once we leave the current group or flat-category band —
    // we don't want `sdk/identity` to point at `react/overview` just because
    // it comes later in the array.
    if (current.group !== undefined && candidate.group !== current.group) return null;
    if (current.group === undefined && candidate.category !== current.category) return null;
  }
  return null;
}

export function partitionCategoryForSidebar(category: DocCategory): {
  readonly ungrouped: readonly DocMeta[];
  readonly groups: readonly DocGroupBlock[];
} {
  const docs = docsByCategory(category);
  const ungrouped: DocMeta[] = [];
  const groupMap = new Map<string, DocMeta[]>();

  for (const doc of docs) {
    if (doc.group === undefined) {
      ungrouped.push(doc);
      continue;
    }
    const existing = groupMap.get(doc.group);
    if (existing) {
      existing.push(doc);
    } else {
      groupMap.set(doc.group, [doc]);
    }
  }

  const groups: DocGroupBlock[] = [];
  for (const [key, groupDocs] of groupMap) {
    groups.push({
      key,
      label: GROUP_META[key] ?? key,
      docs: groupDocs,
    });
  }

  return { ungrouped, groups };
}
