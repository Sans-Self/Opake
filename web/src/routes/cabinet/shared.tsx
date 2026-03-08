import { createFileRoute } from "@tanstack/react-router";
import { UsersIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";

function SharedPage() {
  const breadcrumbs = (
    <div className="breadcrumbs text-ui min-w-0 flex-1 overflow-hidden">
      <ul>
        <li>
          <span className="text-base-content font-medium">Shared with me</span>
        </li>
      </ul>
    </div>
  );

  return (
    <PanelShell depth={1} breadcrumbs={breadcrumbs} footer="Shared items · Encrypted">
      <div
        role="alert"
        className="alert border-success/30 bg-bg-sage mx-4 mt-4 gap-2.5 rounded-xl p-3"
      >
        <UsersIcon size={13} className="text-success mt-0.5 shrink-0" />
        <div>
          <div className="text-success mb-0.5 text-xs font-medium">
            Shared via decentralised identity
          </div>
          <div className="text-caption text-success/80 leading-relaxed">
            Files shared via DID. Encrypted in transit and at rest — only invited parties can
            decrypt.
          </div>
        </div>
      </div>

      <div className="hero py-16">
        <div className="hero-content flex-col text-center">
          <div className="bg-bg-sage flex size-13 items-center justify-center rounded-[14px]">
            <UsersIcon size={22} className="text-success" />
          </div>
          <div className="text-ui text-text-muted">No shared files yet</div>
        </div>
      </div>
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/shared")({
  component: SharedPage,
});
