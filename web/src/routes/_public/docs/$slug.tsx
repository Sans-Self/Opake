import type { ComponentType } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { MdxContent } from "@/components/content/MdxProvider";
import { findDoc } from "@/lib/docs-registry";
import { ogMeta } from "@/lib/og-meta";

import GettingStarted from "@/content/docs/getting-started.mdx";
import AtProtocol from "@/content/docs/at-protocol.mdx";
import EncryptionKeys from "@/content/docs/encryption-keys.mdx";
import SharingDids from "@/content/docs/sharing-dids.mdx";
import Keyrings from "@/content/docs/keyrings.mdx";
import SeedPhrase from "@/content/docs/seed-phrase.mdx";
import Pairing from "@/content/docs/pairing.mdx";
import Cli from "@/content/docs/cli.mdx";
import Glossary from "@/content/docs/glossary.mdx";
import Faq from "@/content/faq.mdx";

const CONTENT_BY_SLUG: Partial<
  Record<string, ComponentType<{ readonly components?: Record<string, ComponentType<never>> }>>
> = {
  "getting-started": GettingStarted,
  "at-protocol": AtProtocol,
  "encryption-keys": EncryptionKeys,
  "sharing-dids": SharingDids,
  keyrings: Keyrings,
  "seed-phrase": SeedPhrase,
  pairing: Pairing,
  cli: Cli,
  glossary: Glossary,
  faq: Faq,
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
    <div className="mx-auto max-w-3xl px-6 pt-28 pb-20 sm:px-10">
      <MdxContent Content={Content} className="prose" />
    </div>
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
