import type { ComponentType } from "react";
import { createLazyFileRoute, Link } from "@tanstack/react-router";
import { ArrowLeftIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { MdxContent } from "@/components/content/MdxProvider";
import { DocsSidebarCabinet } from "@/components/content/docs-sidebar";
import { findDoc } from "@/lib/docs-registry";

import SdkOverview from "@/content/docs/build/sdk/overview.mdx";
import SdkAuthentication from "@/content/docs/build/sdk/authentication.mdx";
import SdkIdentity from "@/content/docs/build/sdk/identity.mdx";
import SdkFiles from "@/content/docs/build/sdk/files.mdx";
import SdkSharing from "@/content/docs/build/sdk/sharing.mdx";
import SdkWorkspaces from "@/content/docs/build/sdk/workspaces.mdx";
import SdkEvents from "@/content/docs/build/sdk/events.mdx";
import SdkStorage from "@/content/docs/build/sdk/storage.mdx";
import ReactOverview from "@/content/docs/build/react/overview.mdx";
import ReactQueries from "@/content/docs/build/react/queries.mdx";
import ReactMutations from "@/content/docs/build/react/mutations.mdx";
import ReactLiveUpdates from "@/content/docs/build/react/live-updates.mdx";

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
  "sdk/authentication": SdkAuthentication,
  "sdk/identity": SdkIdentity,
  "sdk/files": SdkFiles,
  "sdk/sharing": SdkSharing,
  "sdk/workspaces": SdkWorkspaces,
  "sdk/events": SdkEvents,
  "sdk/storage": SdkStorage,
  "react/overview": ReactOverview,
  "react/queries": ReactQueries,
  "react/mutations": ReactMutations,
  "react/live-updates": ReactLiveUpdates,
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
      <div className="flex gap-6 p-6">
        <aside className="hidden w-44 shrink-0 md:block">
          <div className="sticky top-0">
            <DocsSidebarCabinet currentSlug={meta.slug} currentGroup={category} />
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

export const Route = createLazyFileRoute("/cabinet/docs/$category/$slug")({
  component: NestedDocChapterPage,
});
