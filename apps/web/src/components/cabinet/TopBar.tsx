import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  MagnifyingGlassIcon,
  XIcon,
  BellIcon,
  KeyIcon,
  GearIcon,
  SignOutIcon,
} from "@phosphor-icons/react";
import { Link } from "@tanstack/react-router";
import { DropdownMenu } from "@/components/DropdownMenu";
import { useAuthStore } from "@/stores/auth";
import { useSearchInput } from "@/hooks/useSearchInput";
import { truncateDid } from "@/lib/format";

export function TopBar() {
  const {
    query: searchQuery,
    handleChange: handleSearchChange,
    handleClear: handleSearchClear,
  } = useSearchInput();
  const session = useAuthStore((s) => s.session);
  // eslint-disable-next-line @typescript-eslint/unbound-method -- Zustand actions don't use `this`
  const logout = useAuthStore((s) => s.logout);

  const handle = session.status === "active" ? session.handle : null;
  const did = session.status === "active" ? session.did : null;
  const avatarUrl = session.status === "active" ? session.avatarUrl : null;
  const bannerUrl = session.status === "active" ? session.bannerUrl : null;
  const initial = handle?.[0]?.toUpperCase() ?? "?";

  const [menuOpen, setMenuOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);

  const closeMenu = useCallback(() => setMenuOpen(false), []);

  useEffect(() => {
    if (!menuOpen) return;
    function onClickOutside(e: MouseEvent) {
      const target = e.target as Node;
      if (triggerRef.current?.contains(target) || popoverRef.current?.contains(target)) {
        return;
      }
      setMenuOpen(false);
    }
    document.addEventListener("mousedown", onClickOutside);
    return () => document.removeEventListener("mousedown", onClickOutside);
  }, [menuOpen]);

  return (
    <header className="border-base-300/50 bg-base-300/90 flex shrink-0 items-center gap-3 border-b px-5 py-2.5 backdrop-blur-[10px]">
      {/* Search — hidden on mobile (lives in sidebar instead) */}
      <label className="input input-bordered border-base-300/50 bg-base-100/80 text-ui hidden max-w-90 flex-1 items-center gap-2 rounded-lg py-1.75 md:flex">
        <MagnifyingGlassIcon size={13} className="text-text-faint" />
        <input
          type="text"
          placeholder="Search your cabinet…"
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

      <div className="flex-1" />

      {/* Notifications */}
      <DropdownMenu
        trigger={<BellIcon size={15} className="text-text-muted" />}
        triggerClassName="btn btn-ghost btn-sm btn-square rounded-lg"
        items={[]}
        emptyLabel="No notifications"
      />

      {/* User menu */}
      <button
        ref={triggerRef}
        onClick={() => setMenuOpen((prev) => !prev)}
        className="btn btn-ghost btn-sm gap-2 rounded-lg pl-1"
        aria-expanded={menuOpen}
        aria-haspopup="true"
      >
        {avatarUrl ? (
          <img
            src={avatarUrl}
            alt=""
            className="size-7 shrink-0 rounded-full object-cover"
            aria-hidden="true"
          />
        ) : (
          <div
            className="bg-accent text-caption text-primary flex size-7 items-center justify-center rounded-full font-semibold"
            aria-hidden="true"
          >
            {initial}
          </div>
        )}
        <span className="text-secondary text-xs font-normal">{handle ?? "Not signed in"}</span>
      </button>
      {menuOpen &&
        createPortal(
          <div
            ref={popoverRef}
            className="border-base-300/50 bg-base-100 shadow-panel-lg fixed top-12 right-4 z-9999 w-52.5 rounded-xl border"
          >
            {handle && did && (
              <>
                <div className="relative h-20 w-full overflow-hidden rounded-t-xl">
                  <div
                    className="bg-base-300/60 absolute inset-0 bg-cover bg-center"
                    style={bannerUrl ? { backgroundImage: `url(${bannerUrl})` } : undefined}
                  />
                  <div className="from-base-100 absolute inset-0 bg-linear-to-t to-transparent" />
                </div>
                <div className="border-base-300/50 border-b px-3.5 pt-2 pb-2.5">
                  <div className="text-ui text-base-content font-medium">{handle}</div>
                  <div className="text-caption text-text-faint mt-0.5">{truncateDid(did)}</div>
                </div>
              </>
            )}
            <ul className="menu w-full p-1">
              {[
                { icon: KeyIcon, label: "Encryption Keys", to: "/devices" as const },
                { icon: GearIcon, label: "Settings", to: "/cabinet/settings" as const },
              ].map(({ icon: Icon, label, to }) => (
                <li key={label}>
                  <Link to={to} onClick={closeMenu} className="text-secondary gap-2.5 text-xs">
                    <Icon size={13} />
                    {label}
                  </Link>
                </li>
              ))}
            </ul>
            <div className="divider my-0.5" />
            <ul className="menu w-full p-1 pt-0">
              <li>
                <button
                  onClick={() => {
                    closeMenu();
                    void logout().then(() => {
                      window.location.href = "/devices";
                    });
                  }}
                  className="text-error gap-2.5 text-xs"
                >
                  <SignOutIcon size={13} />
                  Sign out
                </button>
              </li>
            </ul>
          </div>,
          document.body,
        )}
    </header>
  );
}
