import {
  FolderIcon,
  LockIcon,
  UsersIcon,
  BookOpenIcon,
  TrashIcon,
  GearIcon,
} from "@phosphor-icons/react";
import { Link } from "@tanstack/react-router";
import { OpakeLogo } from "../OpakeLogo";
import { SidebarItem } from "./SidebarItem";

const MAIN_NAV = [
  { to: "/cabinet/files" as const, icon: FolderIcon, label: "The Cabinet" },
  { to: "/cabinet/encrypted" as const, icon: LockIcon, label: "Encrypted" },
  { to: "/cabinet/shared" as const, icon: UsersIcon, label: "Shared with me", badge: "4" },
];

const BOTTOM_NAV = [
  { to: "/cabinet/docs" as const, icon: BookOpenIcon, label: "Docs & Help" },
  { to: "/cabinet/trash" as const, icon: TrashIcon, label: "Trash" },
  { to: "/cabinet/settings" as const, icon: GearIcon, label: "Settings" },
];

const WORKSPACES = [
  { id: "ws-personal", name: "Personal", count: 3 },
  { id: "ws-team", name: "Team Alpha", count: 2 },
];

export function Sidebar() {
  return (
    <aside className="border-base-300/50 bg-base-200 flex w-53 shrink-0 flex-col border-r px-3 py-4">
      {/* Logo */}
      <div className="mb-5 px-0.5">
        <Link to="/" className="inline-block">
          <OpakeLogo />
        </Link>
      </div>

      {/* Storage */}
      <div className="mb-5 px-1">
        <div className="text-caption text-text-faint mb-1.5 flex justify-between">
          <span>Storage</span>
          <span>3.1 / 10 GB</span>
        </div>
        <progress className="progress progress-primary h-0.75 w-full" value={31} max={100} />
      </div>

      <div className="divider mx-1 my-0" />

      {/* Main nav */}
      <nav className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto">
        {MAIN_NAV.map(({ to, icon, label, badge }) => (
          <SidebarItem key={to} to={to} icon={icon} label={label} badge={badge} />
        ))}

        {/* Workspaces */}
        <div className="text-label text-text-faint mt-3.5 mb-1.5 ml-1 tracking-widest uppercase">
          Workspaces
        </div>
        {WORKSPACES.map((ws) => (
          <button
            key={ws.id}
            className="text-ui text-text-muted hover:bg-bg-hover flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.75 text-left"
          >
            <div className="bg-accent text-micro text-primary flex size-5 shrink-0 items-center justify-center rounded-md font-semibold">
              {ws.name[0]}
            </div>
            <span className="flex-1">{ws.name}</span>
            <span className="text-label text-text-faint">{ws.count}</span>
          </button>
        ))}
      </nav>

      {/* Bottom nav */}
      <div>
        <div className="divider mx-1 my-0" />
        <div className="flex flex-col gap-0.5">
          {BOTTOM_NAV.map(({ to, icon, label }) => (
            <SidebarItem key={to} to={to} icon={icon} label={label} />
          ))}
        </div>
      </div>
    </aside>
  );
}
