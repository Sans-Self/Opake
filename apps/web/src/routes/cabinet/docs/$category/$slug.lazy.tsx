import type { ComponentType } from "react";
import { createLazyFileRoute, Link } from "@tanstack/react-router";
import { ArrowLeftIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { MdxContent } from "@/components/content/MdxProvider";
import { findDoc } from "@/lib/docs-registry";

import SdkOverview from "@/content/docs/build/sdk/overview.mdx";

/**
 * Nested cabinet docs route: /cabinet/docs/{group}/{slug}.
 * Import list is kept small and deliberate — only pages that live inside
 * a group (sdk, react) register here. Flat docs stay on $slug.lazy.tsx.
 */
type MdxComponent = ComponentType<{
  readonly components?: Record<string, ComponentType<never>>;
}>;

const CONTENT_BY_PATH: Partial<Record<string, MdxComponent>> = {
  "sdk/overview": SdkOverview,
};

function NestedDocChapterPage() {
  const { category, slug } = Route.useParams();
  const meta = findDoc(slug);
  const Content = CONTENT_BY_PATH[`${category}/${slug}`];

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

export const Route = createLazyFileRoute("/cabinet/docs/$category/$slug")({
  component: NestedDocChapterPage,
});
