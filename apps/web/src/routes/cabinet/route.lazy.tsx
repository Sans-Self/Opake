import { useCallback, useEffect, useRef, useState } from "react";
import { createLazyFileRoute, Outlet } from "@tanstack/react-router";
import { Sidebar } from "@/components/cabinet/Sidebar";
import { TopBar } from "@/components/cabinet/TopBar";
import {
  CreateWorkspaceDialog,
  type CreateWorkspaceDialogHandle,
} from "@/components/cabinet/CreateWorkspaceDialog";
import { useWorkspaceStore } from "@/stores/workspace";
import { getOpake, useAuthStore } from "@/stores/auth";
import { taskStore } from "@/stores/tasks";
import { startDaemon } from "@opake/daemon";
import { Opake } from "@opake/sdk";
import { toastError, toastSuccess } from "@/stores/toast";

function CabinetLayout() {
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const dialogRef = useRef<CreateWorkspaceDialogHandle>(null);

  // Subscribe to the WASM WorkspaceKeeper on mount. The store installs
  // a watcher and (if not already loaded) kicks off the bootstrap
  // fetch that populates it. SSE events keep the store live thereafter.
  // On unmount, detach the watcher so a remount reinstalls a fresh one
  // (the previous keeper was wiped by stopSseConsumer). State is kept
  // so the sidebar doesn't flash empty during the unmount/remount cycle.
  useEffect(() => {
    useWorkspaceStore.getState().subscribe();
    return () => {
      useWorkspaceStore.getState().detachWatcher();
    };
  }, []);

  // Start the WASM SSE consumer; stop it on teardown so
  // `TreeKeeper::uninstall_all` runs and the previous user's
  // `ContentKey`s / decrypted names don't linger across login.
  //
  // The appview URL is resolved inside WASM from the stored config —
  // seeded at boot via `setDefaultAppviewUrl`. No env read here.
  useEffect(() => {
    const opake = getOpake();
    void opake.startSseConsumer().catch((err: unknown) => {
      console.warn("[opake] startSseConsumer failed:", err);
    });
    return () => {
      try {
        opake.stopSseConsumer();
      } catch (err) {
        console.warn("[opake] stopSseConsumer failed:", err);
      }
    };
  }, []);

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
    return () => handle?.stop();
  }, []);

  // Reset workspace store only when session transitions away from active
  useEffect(() => {
    // eslint-disable-next-line functional/no-let
    let prevStatus = useAuthStore.getState().session.status;
    return useAuthStore.subscribe((state) => {
      const status = state.session.status;
      if (prevStatus === "active" && status !== "active") {
        useWorkspaceStore.getState().reset();
      }
      prevStatus = status;
    });
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

  const handleCreateWorkspace = useCallback((name: string, description: string | undefined) => {
    void useWorkspaceStore
      .getState()
      .createWorkspace(name, description)
      .then(() => toastSuccess("Workspace created"))
      .catch((err: unknown) => {
        toastError(err instanceof Error ? err.message : "Failed to create workspace");
      });
  }, []);

  return (
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

      <CreateWorkspaceDialog ref={dialogRef} onConfirm={handleCreateWorkspace} />
    </div>
  );
}

export const Route = createLazyFileRoute("/cabinet")({
  component: CabinetLayout,
});
