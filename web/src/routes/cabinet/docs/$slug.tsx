import type { ComponentType } from "react";
import { createFileRoute, Link } from "@tanstack/react-router";
import { ArrowLeftIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { MdxContent } from "@/components/content/MdxProvider";
import { findDoc } from "@/lib/docs-registry";

import GettingStarted from "@/content/docs/getting-started.mdx";
import AtProtocol from "@/content/docs/at-protocol.mdx";
import EncryptionKeys from "@/content/docs/encryption-keys.mdx";
import SharingDids from "@/content/docs/sharing-dids.mdx";
import Keyrings from "@/content/docs/keyrings.mdx";
import Pairing from "@/content/docs/pairing.mdx";
import Glossary from "@/content/docs/glossary.mdx";
import Faq from "@/content/faq.mdx";

type MdxComponent = ComponentType<{
  readonly components?: Record<string, ComponentType<never>>;
}>;

const CONTENT_BY_SLUG: Partial<Record<string, MdxComponent>> = {
  "getting-started": GettingStarted,
  "at-protocol": AtProtocol,
  "encryption-keys": EncryptionKeys,
  "sharing-dids": SharingDids,
  keyrings: Keyrings,
  pairing: Pairing,
  glossary: Glossary,
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
      <div className="overflow-y-auto p-6">
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
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/docs/$slug")({
  component: DocChapterPage,
});
