import { useCallback, useEffect, useRef, useState } from "react";
import { createLazyFileRoute, Outlet } from "@tanstack/react-router";
import { OpakeProvider } from "@opake/react";
import { Sidebar } from "@/components/cabinet/Sidebar";
import { TopBar } from "@/components/cabinet/TopBar";
import {
  CreateWorkspaceDialog,
  type CreateWorkspaceDialogHandle,
} from "@/components/cabinet/CreateWorkspaceDialog";
import { clearPreviewCache } from "@/components/cabinet/FilePreview";
import { evictAllReadmeCaches } from "@/components/cabinet/DirectoryReadme";
import { getOpake, useAuthStore } from "@/stores/auth";
import { taskStore } from "@/stores/tasks";
import { startDaemon } from "@opake/daemon";
import { Opake } from "@opake/sdk";

function CabinetLayout() {
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const dialogRef = useRef<CreateWorkspaceDialogHandle>(null);

  // WorkspaceKeeper lifecycle (subscribe / bootstrap / wipe) is owned by
  // @opake/react's useWorkspaces + the OpakeProvider's SSE lifecycle —
  // no per-route wiring needed.

  // SSE consumer lifecycle is handled by <OpakeProvider> below, which
  // calls startSseConsumer on mount and (on unmount) stopSseConsumer
  // followed by wipeState — the latter drains the keepers so the previous
  // user's `ContentKey`s / decrypted names don't linger across login.

  // JS-side decrypted-plaintext caches (preview + readme Suspense maps)
  // live at module scope in their respective components so they survive
  // Suspense unmount/remount cycles. WASM's wipeState clears the keepers
  // but can't reach these — drain them here when the cabinet layout
  // tears down (logout, session switch, auth-gate redirect).
  useEffect(
    () => () => {
      clearPreviewCache();
      evictAllReadmeCaches();
    },
    [],
  );

  // Background daemon — timer polling for maintenance tasks only.
  useEffect(() => {
    // eslint-disable-next-line functional/no-let -- handle assigned inside async IIFE
    let handle: ReturnType<typeof startDaemon> | null = null;
    void Opake.taskDefs().then((defs) => {
      handle = startDaemon(getOpake(), defs, taskStore, {
        onSessionExpired: () => {
          handle?.stop();
          void useAuthStore.getState().logout();
        },
      });
    });

    // Dev/test-only drain handle: the web tier's maintenance timers are
    // best-effort and slow (the share-retry tick is 300s with no leading
    // fire), so a cross-tier e2e can't observe the drain by waiting. This
    // exposes the existing `retryPendingShares` entry point so the test can
    // drive the drain on demand and race it against the CLI runner. It adds
    // no machinery and is stripped from production builds — the only effect
    // is making the already-present drain callable.
    if (import.meta.env.DEV) {
      (
        window as unknown as {
          __opakeMaintenance?: { retryPendingShares: () => Promise<unknown> };
        }
      ).__opakeMaintenance = {
        retryPendingShares: () => getOpake().retryPendingShares(),
      };
    }

    return () => {
      handle?.stop();
      if (import.meta.env.DEV) {
        delete (window as unknown as { __opakeMaintenance?: unknown }).__opakeMaintenance;
      }
    };
  }, []);

  // Scroll lock + Escape handler when mobile drawer is open
  useEffect(() => {
    if (!sidebarOpen) return;

    document.body.style.overflow = "hidden";

    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setSidebarOpen(false);
    }
    document.addEventListener("keydown", onKeyDown);

    return () => {
      document.body.style.overflow = "";
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [sidebarOpen]);

  const closeSidebar = useCallback(() => setSidebarOpen(false), []);
  const toggleSidebar = useCallback(() => setSidebarOpen((prev) => !prev), []);

  // Creation, in-flight state, and the visibility wait for the SSE echo are
  // owned by CreateWorkspaceDialog — it must run inside OpakeProvider to
  // observe the workspace keeper, so the whole flow lives there.

  return (
    <OpakeProvider opake={getOpake()}>
      <div className="bg-base-300 flex h-screen overflow-hidden font-sans">
        {/* Desktop sidebar */}
        <div className="hidden md:flex">
          <Sidebar onCreateWorkspace={() => dialogRef.current?.show()} />
        </div>

        {/* Mobile sidebar drawer */}
        {sidebarOpen && (
          <div
            className="fixed inset-0 z-40 flex md:hidden"
            role="dialog"
            aria-modal="true"
            aria-label="Navigation"
          >
            {/* Backdrop */}
            <div
              className="absolute inset-0 bg-black/40 transition-opacity"
              onClick={closeSidebar}
              aria-hidden="true"
            />
            {/* Drawer */}
            <div className="relative z-10">
              <Sidebar
                onNavigate={closeSidebar}
                onCreateWorkspace={() => {
                  closeSidebar();
                  dialogRef.current?.show();
                }}
              />
            </div>
          </div>
        )}

        {/* Main content area */}
        <main className="flex flex-1 flex-col overflow-hidden">
          <TopBar onMenuToggle={toggleSidebar} menuOpen={sidebarOpen} />
          <div className="flex-1 overflow-hidden">
            <Outlet />
          </div>
        </main>

        <CreateWorkspaceDialog ref={dialogRef} />
      </div>
    </OpakeProvider>
  );
}

export const Route = createLazyFileRoute("/cabinet")({
  component: CabinetLayout,
});
