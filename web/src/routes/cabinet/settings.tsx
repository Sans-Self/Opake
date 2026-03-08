import { createFileRoute } from "@tanstack/react-router";
import {
  UserIcon,
  LockIcon,
  ShareNetworkIcon,
  ShieldCheckIcon,
  BellIcon,
  CaretRightIcon,
} from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";

const SETTINGS_SECTIONS = [
  { label: "Account & Identity", desc: "DID: did:plc:7f2ab3c4d\u20268e91f0", icon: UserIcon },
  { label: "Encryption Keys", desc: "Last rotated 14 days ago \u00b7 Active", icon: LockIcon },
  {
    label: "Sharing & Permissions",
    desc: "3 active collaborators",
    icon: ShareNetworkIcon,
  },
  { label: "Connected Devices", desc: "2 devices linked", icon: ShieldCheckIcon },
  { label: "Notifications", desc: "Email & in-app alerts", icon: BellIcon },
];

function SettingsPage() {
  const breadcrumbs = (
    <div className="breadcrumbs text-ui min-w-0 flex-1 overflow-hidden">
      <ul>
        <li>
          <span className="text-base-content font-medium">Settings</span>
        </li>
      </ul>
    </div>
  );

  return (
    <PanelShell depth={1} breadcrumbs={breadcrumbs} footer="Account settings">
      <div className="p-5">
        <div className="mb-5">
          <div className="text-ui text-base-content mb-1 font-medium">Settings</div>
          <div className="text-text-muted text-xs">Manage your account, keys, and preferences.</div>
        </div>
        <div className="divider mt-0 mb-4" />
        <div className="flex flex-col gap-1.5">
          {SETTINGS_SECTIONS.map(({ label, desc, icon: Icon }) => (
            <div
              key={label}
              className="card card-bordered border-base-300/50 bg-base-100 cursor-pointer p-3.5"
            >
              <div className="bg-bg-stone flex size-8 shrink-0 items-center justify-center rounded-lg">
                <Icon size={14} className="text-text-muted" />
              </div>
              <div className="flex-1">
                <div className="text-ui text-base-content font-medium">{label}</div>
                <div className="text-caption text-text-muted">{desc}</div>
              </div>
              <CaretRightIcon size={13} className="text-text-faint" />
            </div>
          ))}
        </div>
      </div>
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/settings")({
  component: SettingsPage,
});
