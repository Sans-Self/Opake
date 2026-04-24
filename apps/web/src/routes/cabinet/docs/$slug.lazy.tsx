import type { ComponentType } from "react";
import { createLazyFileRoute, Link } from "@tanstack/react-router";
import { ArrowLeftIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { MdxContent } from "@/components/content/MdxProvider";
import { DocsSidebarCabinet } from "@/components/content/docs-sidebar";
import { findDoc } from "@/lib/docs-registry";

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

type MdxComponent = ComponentType<{
  readonly components?: Record<string, ComponentType<never>>;
}>;

const CONTENT_BY_SLUG: Partial<Record<string, MdxComponent>> = {
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
};

function DocChapterPage() {
  const { slug } = Route.useParams();
  const meta = findDoc(slug);
  const Content = CONTENT_BY_SLUG[slug];

  if (!meta || !Content) {
    return (
      <PanelShell depth={1} breadcrumbs={<span />} footer="Documentation · Opake">
        <div className="flex flex-col items-center gap-4 p-10">
          <p className="text-text-muted">Page not found.</p>
          <Link to="/cabinet/docs" className="btn btn-neutral btn-sm">
            Back to docs
          </Link>
        </div>
      </PanelShell>
    );
  }

  const breadcrumbs = (
    <div className="breadcrumbs text-ui min-w-0 flex-1 overflow-hidden">
      <ul>
        <li>
          <Link to="/cabinet/docs" className="text-text-muted hover:text-base-content">
            Docs & Help
          </Link>
        </li>
        <li>
          <span className="text-base-content font-medium">{meta.title}</span>
        </li>
      </ul>
    </div>
  );

  return (
    <PanelShell depth={1} breadcrumbs={breadcrumbs} footer={`${meta.title} · Opake`}>
      <div className="flex gap-6 p-6">
        <aside className="hidden w-44 shrink-0 md:block">
          <div className="sticky top-0">
            <DocsSidebarCabinet currentSlug={meta.slug} />
          </div>
        </aside>
        <div className="min-w-0 flex-1">
          <MdxContent Content={Content} className="prose max-w-none text-sm leading-relaxed" />
          <div className="border-border-accent/30 mt-10 border-t pt-4">
            <Link
              to="/cabinet/docs"
              className="text-text-muted hover:text-primary text-ui inline-flex items-center gap-1.5 transition-colors"
            >
              <ArrowLeftIcon size={12} />
              Back to docs
            </Link>
          </div>
        </div>
      </div>
    </PanelShell>
  );
}

export const Route = createLazyFileRoute("/cabinet/docs/$slug")({
  component: DocChapterPage,
});
