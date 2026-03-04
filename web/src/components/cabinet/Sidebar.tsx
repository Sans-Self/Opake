import {
  Folder,
  Lock,
  Users,
  Star,
  BookOpen,
  Trash,
  Gear,
} from "@phosphor-icons/react";
import { Link } from "@tanstack/react-router";
import { OpakeLogo } from "../OpakeLogo";
import { SidebarItem } from "./SidebarItem";
import type { PanelType } from "./types";

const MAIN_NAV = [
  { type: "root" as const, icon: Folder, label: "The Cabinet" },
  { type: "encrypted" as const, icon: Lock, label: "Encrypted" },
  { type: "shared" as const, icon: Users, label: "Shared with me", badge: "4" },
  { type: "starred" as const, icon: Star, label: "Starred" },
];

const BOTTOM_NAV = [
  { type: "docs" as const, icon: BookOpen, label: "Docs & Help" },
  { type: "trash" as const, icon: Trash, label: "Trash" },
  { type: "settings" as const, icon: Gear, label: "Settings" },
];

const WORKSPACES = [
  { id: "ws-personal", name: "Personal", count: 3 },
  { id: "ws-team", name: "Team Alpha", count: 2 },
];

interface SidebarProps {
  activePanelType: PanelType;
  panelDepth: number;
  onOpenSection: (type: PanelType, title: string) => void;
}

export function Sidebar({
  activePanelType,
  panelDepth,
  onOpenSection,
}: SidebarProps) {
  return (
    <aside className="flex w-[212px] shrink-0 flex-col border-r border-base-300/50 bg-base-200 px-3 py-4">
      {/* Logo */}
      <div className="mb-5 px-0.5">
        <Link to="/" className="inline-block">
          <OpakeLogo />
        </Link>
      </div>

      {/* Storage */}
      <div className="mb-5 px-1">
        <div className="mb-1.5 flex justify-between text-[11px] text-text-faint">
          <span>Storage</span>
          <span>3.1 / 10 GB</span>
        </div>
        <progress
          className="progress progress-primary h-[3px] w-full"
          value={31}
          max={100}
        />
      </div>

      <div className="divider my-0 mx-1" />

      {/* Main nav */}
      <nav className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto">
        {MAIN_NAV.map(({ type, icon, label, badge }) => (
          <SidebarItem
            key={type}
            icon={icon}
            label={label}
            badge={badge}
            active={activePanelType === type && panelDepth === 1}
            onClick={() => onOpenSection(type, label)}
          />
        ))}

        {/* Workspaces */}
        <div className="mt-3.5 mb-1.5 ml-1 text-[10px] uppercase tracking-[0.1em] text-text-faint">
          Workspaces
        </div>
        {WORKSPACES.map((ws) => (
          <button
            key={ws.id}
            className="flex w-full items-center gap-2.5 rounded-[9px] px-2.5 py-[7px] text-left text-[13px] text-text-muted hover:bg-bg-hover"
          >
            <div className="flex size-5 shrink-0 items-center justify-center rounded-[5px] bg-accent text-[9px] font-semibold text-primary">
              {ws.name[0]}
            </div>
            <span className="flex-1">{ws.name}</span>
            <span className="text-[10px] text-text-faint">{ws.count}</span>
          </button>
        ))}
      </nav>

      {/* Bottom nav */}
      <div>
        <div className="divider my-0 mx-1" />
        <div className="flex flex-col gap-0.5">
          {BOTTOM_NAV.map(({ type, icon, label }) => (
            <SidebarItem
              key={type}
              icon={icon}
              label={label}
              active={activePanelType === type}
              onClick={() => onOpenSection(type, label)}
            />
          ))}
        </div>
      </div>
    </aside>
  );
}
