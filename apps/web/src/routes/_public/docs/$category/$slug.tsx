import type { ComponentType } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { MdxContent } from "@/components/content/MdxProvider";
import { DocsSidebar } from "@/components/content/docs-sidebar";
import { findDoc } from "@/lib/docs-registry";
import { ogMeta } from "@/lib/og-meta";

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
 * Nested docs route: /docs/{group}/{slug} — used by pages inside a
 * subsection (sdk, react). The flat `/docs/{slug}` route handles
 * everything else; the two routes coexist because TanStack picks the
 * more specific matcher first.
 */
const CONTENT_BY_PATH: Partial<
  Record<string, ComponentType<{ readonly components?: Record<string, ComponentType<never>> }>>
> = {
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
  const Content = CONTENT_BY_PATH[`${category}/${slug}`];

  if (!Content) {
    return (
      <div className="flex flex-col items-center gap-4 px-6 pt-28 pb-20">
        <p className="text-text-muted">Page not found.</p>
      </div>
    );
  }

  return (
    <div className="mx-auto flex w-full max-w-6xl gap-10 px-6 pt-28 pb-20 sm:px-10">
      <aside className="hidden shrink-0 lg:block lg:w-60">
        <div className="sticky top-24 max-h-[calc(100vh-7rem)] overflow-y-auto pr-1">
          <DocsSidebar currentSlug={slug} currentGroup={category} />
        </div>
      </aside>
      <main className="min-w-0 flex-1">
        <MdxContent Content={Content} className="prose max-w-3xl" />
      </main>
    </div>
  );
}

export const Route = createFileRoute("/_public/docs/$category/$slug")({
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
  component: NestedDocChapterPage,
});
