import {
  FolderIcon,
  UsersIcon,
  BookOpenIcon,
  GearIcon,
  MagnifyingGlassIcon,
  XIcon,
} from "@phosphor-icons/react";
import { Link } from "@tanstack/react-router";
import { OpakeLogo } from "../OpakeLogo";
import { useAppStore } from "@/stores/app";
import { useSearchInput } from "@/hooks/useSearchInput";
import { SidebarItem } from "./SidebarItem";

const MAIN_NAV = [
  { to: "/cabinet/files" as const, icon: FolderIcon, label: "Your Cabinet" },
  { to: "/cabinet/shared" as const, icon: UsersIcon, label: "Sharing" },
];

const BOTTOM_NAV = [
  { to: "/cabinet/docs" as const, icon: BookOpenIcon, label: "Docs & Help" },
  { to: "/cabinet/settings" as const, icon: GearIcon, label: "Settings" },
];

const WORKSPACES = [
  { id: "ws-personal", name: "Personal", count: 3 },
  { id: "ws-team", name: "Team Alpha", count: 2 },
];

interface SidebarProps {
  readonly onNavigate?: () => void;
}

export function Sidebar({ onNavigate }: SidebarProps) {
  const anyLoading = useAppStore((s) => s.anythingLoading());
  const {
    query: searchQuery,
    handleChange: handleSearchChange,
    handleClear: handleSearchClear,
  } = useSearchInput();

  return (
    <aside className="border-base-300/50 bg-base-200 flex h-full w-53 shrink-0 flex-col border-r px-3 py-4">
      {/* Logo — hidden on mobile (shown in mobile topbar instead) */}
      <div className="mb-5 hidden px-0.5 md:block">
        <Link to="/" className="inline-block">
          <OpakeLogo loading={anyLoading} />
        </Link>
      </div>

      {/* Search — mobile only */}
      <label className="input input-bordered border-base-300/50 bg-base-100/80 text-ui mb-3 flex items-center gap-2 rounded-lg py-1.75 md:hidden">
        <MagnifyingGlassIcon size={13} className="text-text-faint" />
        <input
          type="text"
          placeholder="Search…"
          value={searchQuery}
          onChange={(e) => handleSearchChange(e.target.value)}
          className="text-secondary grow bg-transparent"
        />
        {searchQuery && (
          <button onClick={handleSearchClear} className="btn btn-ghost btn-xs text-text-faint p-0">
            <XIcon size={12} />
          </button>
        )}
      </label>

      {/* Main nav */}
      <nav className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto">
        {MAIN_NAV.map(({ to, icon, label }) => (
          <SidebarItem key={to} to={to} icon={icon} label={label} onClick={onNavigate} />
        ))}

        {/* Workspaces */}
        <div className="text-label text-text-faint mt-3.5 mb-1.5 ml-1 tracking-widest uppercase">
          Workspaces
        </div>
        {WORKSPACES.map((ws) => (
          <button
            key={ws.id}
            onClick={onNavigate}
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
            <SidebarItem key={to} to={to} icon={icon} label={label} onClick={onNavigate} />
          ))}
        </div>
      </div>
    </aside>
  );
}
