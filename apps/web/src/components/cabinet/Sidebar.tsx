import {
  FolderIcon,
  UsersIcon,
  BookOpenIcon,
  GearIcon,
  MagnifyingGlassIcon,
  XIcon,
  PlusIcon,
  ArrowsClockwiseIcon,
} from "@phosphor-icons/react";
import { Link, useMatchRoute } from "@tanstack/react-router";
import { OpakeLogo } from "../OpakeLogo";
import { useAppStore } from "@/stores/app";
import { useKeyringStore } from "@/stores/keyring";
import { useSearchInput } from "@/hooks/useSearchInput";
import { SidebarItem } from "./SidebarItem";
import { rkeyFromUri } from "@/lib/atUri";

const MAIN_NAV = [
  { to: "/cabinet/files" as const, icon: FolderIcon, label: "Your Cabinet" },
  { to: "/cabinet/shared" as const, icon: UsersIcon, label: "Sharing" },
];

const BOTTOM_NAV = [
  { to: "/cabinet/docs" as const, icon: BookOpenIcon, label: "Docs & Help" },
  { to: "/cabinet/tasks" as const, icon: ArrowsClockwiseIcon, label: "Tasks" },
  { to: "/cabinet/settings" as const, icon: GearIcon, label: "Settings" },
];

interface SidebarProps {
  readonly onNavigate?: () => void;
  readonly onCreateWorkspace?: () => void;
}

export function Sidebar({ onNavigate, onCreateWorkspace }: SidebarProps) {
  const anyLoading = useAppStore((s) => s.anythingLoading());
  const keyrings = useKeyringStore((s) => s.keyrings);
  const matchRoute = useMatchRoute();
  const {
    query: searchQuery,
    handleChange: handleSearchChange,
    handleClear: handleSearchClear,
  } = useSearchInput();

  const workspaceEntries = Object.values(keyrings);

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
          <button
            onClick={handleSearchClear}
            className="btn btn-ghost btn-xs text-text-faint p-0"
            aria-label="Clear search"
          >
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
        <div className="text-label text-text-faint mt-3.5 mb-1.5 ml-1 flex items-center justify-between tracking-widest uppercase">
          <span>Workspaces</span>
          {onCreateWorkspace && (
            <button
              onClick={onCreateWorkspace}
              className="text-text-faint hover:text-primary transition-colors"
              aria-label="Create workspace"
            >
              <PlusIcon size={12} weight="bold" />
            </button>
          )}
        </div>
        {workspaceEntries.map((ws) => {
          const wsRkey = rkeyFromUri(ws.uri);
          const active = Boolean(
            matchRoute({ to: "/cabinet/workspace/$rkey", params: { rkey: wsRkey }, fuzzy: true }),
          );
          // eslint-disable-next-line @typescript-eslint/prefer-nullish-coalescing -- intentional: catch empty string from PDS
          const displayName = ws.name || "Unnamed";

          return (
            <Link
              key={ws.uri}
              to="/cabinet/workspace/$rkey"
              params={{ rkey: wsRkey }}
              onClick={onNavigate}
              className={`text-ui flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.75 text-left transition-colors ${
                active ? "bg-accent text-primary" : "text-text-muted hover:bg-bg-hover"
              }`}
            >
              {ws.icon ? (
                <img
                  src={`data:image/png;base64,${ws.icon}`}
                  alt=""
                  className="size-5 shrink-0 rounded-md object-cover"
                />
              ) : (
                <div
                  className={`text-micro flex size-5 shrink-0 items-center justify-center rounded-md font-semibold ${
                    active ? "bg-primary/20 text-primary" : "bg-accent text-primary"
                  }`}
                >
                  {displayName[0].toUpperCase()}
                </div>
              )}
              <span className="flex-1 truncate">{displayName}</span>
              {/* [NOI FEEDBACK PLS] — design says three-dots here replacing member count.
                  All actions (members, settings, invite, leave) are in the toolbar already.
                  Keeping member count for now — three-dots in a Link is invalid HTML without restructuring. */}
              <span className="text-label text-text-faint">{ws.member_count}</span>
            </Link>
          );
        })}
        {workspaceEntries.length === 0 && (
          <span className="text-caption text-text-faint ml-1">No workspaces yet</span>
        )}
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
