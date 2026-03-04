import {
  MagnifyingGlass,
  X,
  ShieldCheck,
  Bell,
  User,
  Lock,
  Gear,
  SignOut,
} from "@phosphor-icons/react";
import { Link } from "@tanstack/react-router";

interface TopBarProps {
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onOpenSettings: () => void;
}

function closeDropdown(e: React.MouseEvent) {
  (e.currentTarget.closest("details") as HTMLDetailsElement)?.removeAttribute(
    "open",
  );
}

export function TopBar({
  searchQuery,
  onSearchChange,
  onOpenSettings,
}: TopBarProps) {
  return (
    <header className="flex shrink-0 items-center gap-3 border-b border-base-300/50 bg-base-300/90 px-5 py-2.5 backdrop-blur-[10px]">
      {/* Search */}
      <label className="input input-bordered flex max-w-[360px] flex-1 items-center gap-2 rounded-lg border-base-300/50 bg-base-100/80 py-[7px] text-ui">
        <MagnifyingGlass size={13} className="text-text-faint" />
        <input
          type="text"
          placeholder="Search your cabinet…"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          className="grow bg-transparent text-secondary"
        />
        {searchQuery && (
          <button
            onClick={() => onSearchChange("")}
            className="btn btn-ghost btn-xs p-0 text-text-faint"
          >
            <X size={12} />
          </button>
        )}
      </label>

      <div className="flex-1" />

      {/* E2E badge */}
      <div className="badge badge-outline gap-1.5 border-border-accent bg-accent py-3 text-caption text-primary">
        <ShieldCheck size={12} weight="bold" />
        End-to-end encrypted
      </div>

      {/* Notifications */}
      <div className="indicator">
        <span className="indicator-item badge badge-primary badge-xs size-1.5 p-0" />
        <button className="btn btn-ghost btn-sm btn-square rounded-lg">
          <Bell size={15} className="text-text-muted" />
        </button>
      </div>

      {/* User menu */}
      <details className="dropdown dropdown-end">
        <summary className="btn btn-ghost btn-sm gap-2 rounded-lg pl-1">
          <div className="flex size-7 items-center justify-center rounded-full bg-accent text-caption font-semibold text-primary">
            A
          </div>
          <span className="text-xs font-normal text-secondary">
            alice.bsky.social
          </span>
        </summary>
        <div className="dropdown-content z-50 w-[210px] rounded-xl border border-base-300/50 bg-base-100 shadow-panel-lg">
          <div className="border-b border-base-300/50 px-3.5 py-2.5">
            <div className="text-ui font-medium text-base-content">
              alice.bsky.social
            </div>
            <div className="mt-0.5 text-caption text-text-faint">
              did:plc:7f2ab3c4…8e91
            </div>
          </div>
          <ul className="menu p-1">
            {[
              { icon: User, label: "Profile & DID" },
              { icon: Lock, label: "Encryption Keys" },
              { icon: Gear, label: "Settings" },
            ].map(({ icon: Icon, label }) => (
              <li key={label}>
                <button
                  onClick={(e) => {
                    onOpenSettings();
                    closeDropdown(e);
                  }}
                  className="gap-2.5 text-xs text-secondary"
                >
                  <Icon size={13} />
                  {label}
                </button>
              </li>
            ))}
          </ul>
          <div className="divider my-0.5" />
          <ul className="menu p-1 pt-0">
            <li>
              <Link to="/" className="gap-2.5 text-xs text-error">
                <SignOut size={13} />
                Sign out
              </Link>
            </li>
          </ul>
        </div>
      </details>
    </header>
  );
}
