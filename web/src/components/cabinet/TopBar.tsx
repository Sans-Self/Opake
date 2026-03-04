import { useState, useRef, useEffect } from "react";
import {
  MagnifyingGlass,
  X,
  ShieldCheck,
  Bell,
  User,
  Lock,
  Gear,
  SignOut,
  ShareNetwork,
} from "@phosphor-icons/react";
import { Link } from "@tanstack/react-router";

interface TopBarProps {
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onOpenSettings: () => void;
}

export function TopBar({
  searchQuery,
  onSearchChange,
  onOpenSettings,
}: TopBarProps) {
  const [showUserMenu, setShowUserMenu] = useState(false);
  const userMenuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (
        userMenuRef.current &&
        !userMenuRef.current.contains(e.target as Node)
      ) {
        setShowUserMenu(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  return (
    <header className="flex shrink-0 items-center gap-3 border-b border-base-300/50 bg-base-300/90 px-5 py-2.5 backdrop-blur-[10px]">
      {/* Search */}
      <label className="input input-bordered flex max-w-[360px] flex-1 items-center gap-2 rounded-[9px] border-base-300/50 bg-base-100/80 py-[7px] text-[13px]">
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
      <div className="flex items-center gap-1.5 rounded-lg border border-border-accent bg-accent px-3 py-[5px] text-[11px] text-primary">
        <ShieldCheck size={12} weight="bold" />
        End-to-end encrypted
      </div>

      {/* Notifications */}
      <button className="btn btn-ghost btn-sm btn-square relative rounded-[9px]">
        <Bell size={15} className="text-text-muted" />
        <span className="indicator-item badge badge-primary badge-xs absolute top-1.5 right-1.5 size-1.5 p-0" />
      </button>

      {/* User menu */}
      <div className="relative" ref={userMenuRef}>
        <button
          onClick={() => setShowUserMenu((v) => !v)}
          className="btn btn-ghost btn-sm gap-2 rounded-[9px] pl-1"
        >
          <div className="flex size-7 items-center justify-center rounded-full bg-accent text-[11px] font-semibold text-primary">
            A
          </div>
          <span className="text-[12px] font-normal text-secondary">
            alice.bsky.social
          </span>
        </button>

        {showUserMenu && (
          <div className="menu dropdown-content absolute right-0 top-[calc(100%+8px)] z-50 w-[210px] rounded-[14px] border border-base-300/50 bg-base-100 p-1 shadow-panel-lg">
            <div className="border-b border-base-300/50 px-3.5 py-2.5">
              <div className="text-[13px] font-medium text-base-content">
                alice.bsky.social
              </div>
              <div className="mt-0.5 text-[11px] text-text-faint">
                did:plc:7f2ab3c4…8e91
              </div>
            </div>
            {[
              { icon: User, label: "Profile & DID" },
              { icon: Lock, label: "Encryption Keys" },
              { icon: Gear, label: "Settings" },
            ].map(({ icon: Icon, label }) => (
              <button
                key={label}
                onClick={() => {
                  onOpenSettings();
                  setShowUserMenu(false);
                }}
                className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[12px] text-secondary hover:bg-bg-hover"
              >
                <Icon size={13} />
                {label}
              </button>
            ))}
            <div className="divider my-0.5" />
            <Link
              to="/"
              className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[12px] text-error hover:bg-bg-hover"
            >
              <SignOut size={13} />
              Sign out
            </Link>
          </div>
        )}
      </div>
    </header>
  );
}
