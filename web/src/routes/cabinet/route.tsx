import { useState, useEffect, useCallback } from "react";
import { createFileRoute, redirect, Outlet, Link } from "@tanstack/react-router";
import { Sidebar } from "@/components/cabinet/Sidebar";
import { TopBar } from "@/components/cabinet/TopBar";
import { OpakeLogo } from "@/components/OpakeLogo";
import { useDocumentsStore } from "@/stores/documents";
import { useAuthStore } from "@/stores/auth";
import { useAppStore } from "@/stores/app";

function CabinetLayout() {
  const [searchQuery, setSearchQuery] = useState("");
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const fetchAll = useDocumentsStore((s) => s.fetchAll);
  const anyLoading = useAppStore((s) => s.anythingLoading());

  const toggleSidebar = useCallback(() => setSidebarOpen((v) => !v), []);
  const closeSidebar = useCallback(() => setSidebarOpen(false), []);

  useEffect(() => {
    void fetchAll();
  }, [fetchAll]);

  return (
    <div className="bg-base-300 flex h-screen overflow-hidden font-sans">
      {/* Mobile topbar — logo + hamburger */}
      <div className="border-border-accent/30 bg-base-200 fixed inset-x-0 top-0 z-50 flex items-center justify-between border-b px-4 py-3 md:hidden">
        <Link to="/">
          <OpakeLogo size="sm" loading={anyLoading} />
        </Link>
        <button
          type="button"
          onClick={toggleSidebar}
          aria-label={sidebarOpen ? "Close menu" : "Open menu"}
          className="text-base-content flex size-8 items-center justify-center text-lg"
        >
          {sidebarOpen ? "×" : "="}
        </button>
      </div>

      {/* Backdrop — mobile only */}
      {sidebarOpen && (
        <div
          className="fixed inset-0 z-40 bg-black/40 md:hidden"
          onClick={closeSidebar}
          aria-hidden
        />
      )}

      {/* Sidebar — always visible on desktop, slide-in on mobile */}
      <div
        className={`fixed inset-y-0 left-0 z-40 w-53 pt-14 transition-transform duration-200 ease-out md:static md:pt-0 md:transition-none ${
          sidebarOpen ? "translate-x-0" : "-translate-x-full md:translate-x-0"
        }`}
      >
        <Sidebar
          onNavigate={closeSidebar}
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
        />
      </div>

      <main className="flex flex-1 flex-col overflow-hidden pt-14 md:pt-0">
        <TopBar searchQuery={searchQuery} onSearchChange={setSearchQuery} />
        <Outlet />
      </main>
    </div>
  );
}

export const Route = createFileRoute("/cabinet")({
  ssr: false,
  beforeLoad: async () => {
    // Wait for auth boot if still initializing (IndexedDB session restore)
    const state = useAuthStore.getState();
    if (state.session.status === "initializing") {
      await state.boot();
    }
    if (useAuthStore.getState().session.status !== "active") {
      throw redirect({ to: "/devices/login" });
    }
  },
  component: CabinetLayout,
});
