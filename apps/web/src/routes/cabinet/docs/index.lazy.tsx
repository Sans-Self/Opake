import { createElement } from "react";
import { createLazyFileRoute, Link } from "@tanstack/react-router";
import { ArrowSquareOutIcon, QuestionIcon, SparkleIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { resolveIcon } from "@/components/content/icons";
import { CATEGORY_META, docsByCategory, findDoc, type DocMeta } from "@/lib/docs-registry";

function DocLink({ doc }: { readonly doc: DocMeta }) {
  const className =
    "card card-bordered border-base-300/50 bg-base-100 hover:shadow-panel-sm flex cursor-pointer flex-row items-start gap-3 p-3.5 transition-shadow";
  const content = (
    <>
      <div className="bg-accent flex size-8 shrink-0 items-center justify-center rounded-lg">
        {createElement(resolveIcon(doc.icon), { size: 14, className: "text-primary" })}
      </div>
      <div className="flex-1">
        <div className="text-ui text-base-content mb-0.5 font-medium">{doc.title}</div>
        <div className="text-caption text-text-muted leading-relaxed">{doc.description}</div>
      </div>
      <ArrowSquareOutIcon size={12} className="text-text-faint mt-0.5 shrink-0" />
    </>
  );

  // TanStack's typed Link can't take a runtime-conditional `to`, so we split
  // the two route families explicitly. Docs with a `group` live at the
  // nested `/cabinet/docs/$category/$slug` route; flat docs stay on `$slug`.
  return doc.group ? (
    <Link
      to="/cabinet/docs/$category/$slug"
      params={{ category: doc.group, slug: doc.slug }}
      className={className}
    >
      {content}
    </Link>
  ) : (
    <Link to="/cabinet/docs/$slug" params={{ slug: doc.slug }} className={className}>
      {content}
    </Link>
  );
}

function DocsIndexPage() {
  const breadcrumbs = (
    <div className="breadcrumbs text-ui min-w-0 flex-1 overflow-hidden">
      <ul>
        <li>
          <span className="text-base-content font-medium">Docs & Help</span>
        </li>
      </ul>
    </div>
  );

  const faq = findDoc("faq");

  return (
    <PanelShell depth={1} breadcrumbs={breadcrumbs} footer="Documentation · Opake">
      <div className="space-y-6 p-5">
        {/* Primary CTA — catches visitors who don't yet know which audience fits */}
        <Link
          to="/cabinet/docs/$slug"
          params={{ slug: "getting-started" }}
          className="border-primary/40 bg-base-100 group hover:border-primary/70 hover:shadow-panel-sm flex items-center gap-4 rounded-xl border-2 p-4 transition-all"
        >
          <div className="bg-primary/15 flex size-10 shrink-0 items-center justify-center rounded-lg">
            <SparkleIcon size={18} className="text-primary" />
          </div>
          <div className="flex-1">
            <div className="text-ui text-base-content mb-0.5 font-medium">
              Just getting started?
            </div>
            <div className="text-caption text-text-muted leading-relaxed">
              Set up your cabinet in a few minutes — no prior knowledge required.
            </div>
          </div>
          <ArrowSquareOutIcon size={12} className="text-text-faint shrink-0" />
        </Link>

        {/* Secondary CTA — for visitors with a specific question rather than a task */}
        {faq && (
          <Link
            to="/cabinet/docs/$slug"
            params={{ slug: faq.slug }}
            className="border-border-accent/40 bg-base-100/60 group hover:border-primary/60 hover:bg-base-100 flex items-center gap-3 rounded-xl border p-3 transition-all"
          >
            <div className="bg-accent/60 flex size-8 shrink-0 items-center justify-center rounded-lg">
              <QuestionIcon size={15} className="text-primary" />
            </div>
            <div className="flex-1">
              <span className="text-base-content text-ui font-medium">
                Got a specific question?
              </span>
              <span className="text-text-muted text-ui ml-1.5">Jump into the FAQ.</span>
            </div>
            <ArrowSquareOutIcon size={12} className="text-text-faint shrink-0" />
          </Link>
        )}

        {CATEGORY_META.map((cat) => {
          const docs = docsByCategory(cat.key);
          if (docs.length === 0) return null;
          return (
            <section key={cat.key}>
              <div className="mb-2">
                <div className="text-ui text-base-content font-medium">{cat.label}</div>
                <div className="text-caption text-text-muted">{cat.description}</div>
              </div>
              <div className="flex flex-col gap-2">
                {docs.map((d) => (
                  <DocLink key={d.slug} doc={d} />
                ))}
              </div>
            </section>
          );
        })}
      </div>
    </PanelShell>
  );
}

export const Route = createLazyFileRoute("/cabinet/docs/")({
  component: DocsIndexPage,
});
