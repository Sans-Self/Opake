import type { IconName } from "@/components/content/icons";

export interface DocMeta {
  readonly slug: string;
  readonly title: string;
  readonly description: string;
  readonly icon: IconName;
}

/**
 * Single source of truth for documentation section metadata.
 * Used by both public docs routes and cabinet docs routes.
 */
export const DOCS_REGISTRY: readonly DocMeta[] = [
  {
    slug: "getting-started",
    title: "Getting Started",
    icon: "sparkles",
    description:
      "Set up your cabinet, create your first encrypted file, and explore the interface.",
  },
  {
    slug: "at-protocol",
    title: "AT Protocol",
    icon: "network",
    description: "The open standard powering Opake — identity, data portability, and federation.",
  },
  {
    slug: "encryption-keys",
    title: "Encryption & Keys",
    icon: "lock",
    description: "How end-to-end encryption works in Opake and how your keys are managed.",
  },
  {
    slug: "sharing-dids",
    title: "Sharing & DIDs",
    icon: "share",
    description: "Share files using decentralised identifiers without a central authority.",
  },
  {
    slug: "keyrings",
    title: "Keyrings & Groups",
    icon: "group",
    description: "Manage secure group sharing for families, teams, and research groups.",
  },
  {
    slug: "seed-phrase",
    title: "Your Seed Phrase",
    icon: "seedling",
    description: "Back up and recover your identity with a 24-word recovery phrase.",
  },
  {
    slug: "pairing",
    title: "Multi-Device Magic",
    icon: "pairing",
    description:
      "Securely transfer your identity keypair to new devices using your PDS as a relay.",
  },
  {
    slug: "cli",
    title: "The CLI Manual",
    icon: "terminal",
    description:
      "Complete command reference for the Opake CLI — identity, files, sharing, and more.",
  },
  {
    slug: "glossary",
    title: "Glossary",
    icon: "book",
    description: "A quick-hit reference for the terminology and acronyms we use in Opake.",
  },
  {
    slug: "faq",
    title: "FAQ",
    icon: "question",
    description:
      "Common questions about privacy, security, and how Opake compares to alternatives.",
  },
];

export function findDoc(slug: string): DocMeta | undefined {
  return DOCS_REGISTRY.find((d) => d.slug === slug);
}
