import { createElement } from "react";
import { createLazyFileRoute, Link } from "@tanstack/react-router";
import { ArrowSquareOutIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { resolveIcon } from "@/components/content/icons";
import { DOCS_REGISTRY } from "@/lib/docs-registry";

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

  return (
    <PanelShell depth={1} breadcrumbs={breadcrumbs} footer="Documentation · Opake">
      <div className="p-5">
        <div className="mb-5">
          <div className="text-ui text-base-content mb-1 font-medium">Documentation</div>
          <div className="text-text-muted text-xs">
            Everything you need to get the most out of Opake.
          </div>
        </div>
        <div className="flex flex-col gap-2">
          {DOCS_REGISTRY.map((s) => (
            <Link
              key={s.slug}
              to="/cabinet/docs/$slug"
              params={{ slug: s.slug }}
              className="card card-bordered border-base-300/50 bg-base-100 hover:shadow-panel-sm cursor-pointer p-3.5 transition-shadow"
            >
              <div className="bg-accent flex size-8 shrink-0 items-center justify-center rounded-lg">
                {createElement(resolveIcon(s.icon), { size: 14, className: "text-primary" })}
              </div>
              <div className="flex-1">
                <div className="text-ui text-base-content mb-0.5 font-medium">{s.title}</div>
                <div className="text-caption text-text-muted leading-relaxed">{s.description}</div>
              </div>
              <ArrowSquareOutIcon size={12} className="text-text-faint mt-0.5 shrink-0" />
            </Link>
          ))}
        </div>
      </div>
    </PanelShell>
  );
}

export const Route = createLazyFileRoute("/cabinet/docs/")({
  component: DocsIndexPage,
});
