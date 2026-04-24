import { createFileRoute } from "@tanstack/react-router";
import { ogMeta } from "@/lib/og-meta";
import { MdxContent } from "@/components/content/MdxProvider";
import { DocsSidebar } from "@/components/content/docs-sidebar";
import DocsIndexContent from "@/content/docs/index.mdx";

function DocsIndexPage() {
  return (
    <div className="mx-auto flex w-full max-w-6xl gap-10 px-6 pt-28 pb-20 sm:px-10">
      <aside className="hidden shrink-0 lg:block lg:w-60">
        <div className="sticky top-24 max-h-[calc(100vh-7rem)] overflow-y-auto pr-1">
          <DocsSidebar />
        </div>
      </aside>
      <main className="min-w-0 flex-1">
        <MdxContent Content={DocsIndexContent} />
      </main>
    </div>
  );
}

export const Route = createFileRoute("/_public/docs/")({
  head: () => ({
    meta: ogMeta({
      title: "The Opaque Handbook — Opake",
      description: "Everything you need to get the most out of Opake.",
    }),
  }),
  component: DocsIndexPage,
});
