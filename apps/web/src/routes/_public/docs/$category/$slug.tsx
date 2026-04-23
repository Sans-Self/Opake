import type { ComponentType } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { MdxContent } from "@/components/content/MdxProvider";
import { findDoc } from "@/lib/docs-registry";
import { ogMeta } from "@/lib/og-meta";

import SdkOverview from "@/content/docs/build/sdk/overview.mdx";

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
    <div className="mx-auto max-w-3xl px-6 pt-28 pb-20 sm:px-10">
      <MdxContent Content={Content} className="prose" />
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
