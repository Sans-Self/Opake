import { createFileRoute } from "@tanstack/react-router";
import { LockIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";

function EncryptedPage() {
  const breadcrumbs = (
    <div className="breadcrumbs text-ui min-w-0 flex-1 overflow-hidden">
      <ul>
        <li>
          <span className="text-base-content font-medium">Encrypted</span>
        </li>
      </ul>
    </div>
  );

  return (
    <PanelShell depth={1} breadcrumbs={breadcrumbs} footer="Private items">
      <div
        role="alert"
        className="alert border-border-accent bg-accent mx-4 mt-4 gap-2.5 rounded-xl p-3"
      >
        <LockIcon size={13} className="text-primary mt-0.5 shrink-0" />
        <div>
          <div className="text-accent-content mb-0.5 text-xs font-medium">
            Private encrypted files
          </div>
          <div className="text-caption text-primary leading-relaxed">
            Only you can decrypt these files. Not shared with anyone.
          </div>
        </div>
      </div>

      <div className="hero py-16">
        <div className="hero-content flex-col text-center">
          <div className="bg-accent flex size-13 items-center justify-center rounded-[14px]">
            <LockIcon size={22} className="text-primary" />
          </div>
          <div className="text-ui text-text-muted">No private files yet</div>
        </div>
      </div>
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/encrypted")({
  component: EncryptedPage,
});
