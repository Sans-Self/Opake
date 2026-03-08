import {
  MagnifyingGlassIcon,
  XIcon,
  ShieldCheckIcon,
  BellIcon,
  UserIcon,
  LockIcon,
  GearIcon,
  SignOutIcon,
} from "@phosphor-icons/react";
import { Link } from "@tanstack/react-router";

interface TopBarProps {
  readonly searchQuery: string;
  readonly onSearchChange: (query: string) => void;
}

function closeDropdown(e: React.MouseEvent) {
  e.currentTarget.closest("details")?.removeAttribute("open");
}

export function TopBar({ searchQuery, onSearchChange }: TopBarProps) {
  return (
    <header className="border-base-300/50 bg-base-300/90 flex shrink-0 items-center gap-3 border-b px-5 py-2.5 backdrop-blur-[10px]">
      {/* Search */}
      <label className="input input-bordered border-base-300/50 bg-base-100/80 text-ui flex max-w-90 flex-1 items-center gap-2 rounded-lg py-1.75">
        <MagnifyingGlassIcon size={13} className="text-text-faint" />
        <input
          type="text"
          placeholder="Search your cabinet\u2026"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          className="text-secondary grow bg-transparent"
        />
        {searchQuery && (
          <button
            onClick={() => onSearchChange("")}
            className="btn btn-ghost btn-xs text-text-faint p-0"
          >
            <XIcon size={12} />
          </button>
        )}
      </label>

      <div className="flex-1" />

      {/* E2E badge */}
      <div className="badge badge-outline border-border-accent bg-accent text-caption text-primary gap-1.5 py-3">
        <ShieldCheckIcon size={12} weight="bold" />
        End-to-end encrypted
      </div>

      {/* Notifications */}
      <div className="indicator">
        <span className="indicator-item badge badge-primary badge-xs size-1.5 p-0" />
        <button className="btn btn-ghost btn-sm btn-square rounded-lg">
          <BellIcon size={15} className="text-text-muted" />
        </button>
      </div>

      {/* User menu */}
      <details className="dropdown dropdown-end">
        <summary className="btn btn-ghost btn-sm gap-2 rounded-lg pl-1">
          <div className="bg-accent text-caption text-primary flex size-7 items-center justify-center rounded-full font-semibold">
            A
          </div>
          <span className="text-secondary text-xs font-normal">alice.bsky.social</span>
        </summary>
        <div className="dropdown-content border-base-300/50 bg-base-100 shadow-panel-lg z-50 w-52.5 rounded-xl border">
          <div className="border-base-300/50 border-b px-3.5 py-2.5">
            <div className="text-ui text-base-content font-medium">alice.bsky.social</div>
            <div className="text-caption text-text-faint mt-0.5">did:plc:7f2ab3c4\u20268e91</div>
          </div>
          <ul className="menu p-1">
            {[
              { icon: UserIcon, label: "Profile & DID", to: "/cabinet/settings" as const },
              { icon: LockIcon, label: "Encryption Keys", to: "/cabinet/settings" as const },
              { icon: GearIcon, label: "Settings", to: "/cabinet/settings" as const },
            ].map(({ icon: Icon, label, to }) => (
              <li key={label}>
                <Link to={to} onClick={closeDropdown} className="text-secondary gap-2.5 text-xs">
                  <Icon size={13} />
                  {label}
                </Link>
              </li>
            ))}
          </ul>
          <div className="divider my-0.5" />
          <ul className="menu p-1 pt-0">
            <li>
              <Link to="/" className="text-error gap-2.5 text-xs">
                <SignOutIcon size={13} />
                Sign out
              </Link>
            </li>
          </ul>
        </div>
      </details>
    </header>
  );
}
