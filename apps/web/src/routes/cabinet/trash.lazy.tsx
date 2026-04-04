import { createLazyFileRoute } from "@tanstack/react-router";
import { TrashIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";

function TrashPage() {
  const breadcrumbs = (
    <div className="breadcrumbs text-ui min-w-0 flex-1 overflow-hidden">
      <ul>
        <li>
          <span className="text-base-content font-medium">Trash</span>
        </li>
      </ul>
    </div>
  );

  return (
    <PanelShell depth={1} breadcrumbs={breadcrumbs} footer="Trash · 30 day retention">
      <div className="hero py-16">
        <div className="hero-content flex-col text-center">
          <div className="bg-bg-stone flex size-13 items-center justify-center rounded-[14px]">
            <TrashIcon size={22} className="text-text-faint" />
          </div>
          <div className="text-ui text-text-muted">Trash is empty</div>
          <div className="text-text-faint max-w-60 text-xs leading-relaxed">
            Deleted files appear here for 30 days before permanent removal.
          </div>
        </div>
      </div>
    </PanelShell>
  );
}

export const Route = createLazyFileRoute("/cabinet/trash")({
  component: TrashPage,
});
