import type { ComponentType } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { MdxContent } from "@/components/content/MdxProvider";
import { DocsLayout } from "@/components/content/docs-layout";
import { findDoc } from "@/lib/docs-registry";
import { ogMeta } from "@/lib/og-meta";

import GettingStarted from "@/content/docs/use/getting-started.mdx";
import Pairing from "@/content/docs/use/pairing.mdx";
import SeedPhrase from "@/content/docs/use/seed-phrase.mdx";
import Sharing from "@/content/docs/use/sharing.mdx";
import Workspaces from "@/content/docs/use/workspaces.mdx";
import Troubleshooting from "@/content/docs/use/troubleshooting.mdx";
import Encryption from "@/content/docs/understand/encryption.mdx";
import AtProtocol from "@/content/docs/understand/at-protocol.mdx";
import Glossary from "@/content/docs/understand/glossary.mdx";
import Cli from "@/content/docs/build/cli.mdx";
import Lexicons from "@/content/docs/build/lexicons.mdx";
import Faq from "@/content/docs/faq.mdx";
import Ai from "@/content/docs/ai.mdx";

const CONTENT_BY_SLUG: Partial<
  Record<string, ComponentType<{ readonly components?: Record<string, ComponentType<never>> }>>
> = {
  "getting-started": GettingStarted,
  pairing: Pairing,
  "seed-phrase": SeedPhrase,
  sharing: Sharing,
  workspaces: Workspaces,
  troubleshooting: Troubleshooting,
  encryption: Encryption,
  "at-protocol": AtProtocol,
  glossary: Glossary,
  cli: Cli,
  lexicons: Lexicons,
  faq: Faq,
  ai: Ai,
};

function DocChapterPage() {
  const { slug } = Route.useParams();
  const Content = CONTENT_BY_SLUG[slug];

  if (!Content) {
    return (
      <div className="flex flex-col items-center gap-4 px-6 pt-28 pb-20">
        <p className="text-text-muted">Page not found.</p>
      </div>
    );
  }

  return (
    <DocsLayout currentSlug={slug}>
      <MdxContent Content={Content} className="prose max-w-3xl" />
    </DocsLayout>
  );
}

export const Route = createFileRoute("/_public/docs/$slug")({
  head: ({ params }) => {
    const doc = findDoc(params.slug);
    return {
      meta: ogMeta({
        title: doc ? `${doc.title} — Opake` : "Docs — Opake",
        description: doc?.description ?? "",
        image: doc ? `/og/${params.slug}.png` : undefined,
      }),
    };
  },
  component: DocChapterPage,
});
