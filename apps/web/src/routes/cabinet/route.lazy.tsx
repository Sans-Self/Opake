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
import { useDocumentsStore } from "@/stores/documents/store";
import { taskStore } from "@/stores/tasks";
import { startDaemon } from "@opake/daemon";
import { Opake } from "@opake/sdk";
import { toastError, toastSuccess } from "@/stores/toast";

/** Re-sync the currently viewed directory from the PDS. */
function reloadCurrentDirectory(): void {
  const { loaded, currentDirectoryUri } = useDocumentsStore.getState();
  if (loaded) {
    void useDocumentsStore.getState().loadDirectory(currentDirectoryUri);
  }
}

function CabinetLayout() {
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const dialogRef = useRef<CreateWorkspaceDialogHandle>(null);

  // Load workspaces on mount (deduped by module-level promise)
  useEffect(() => {
    void useWorkspaceStore.getState().loadWorkspaces();
  }, []);

  // Background daemon — PDS maintenance writes + SSE-driven proposal sync.
  // When `sse` is provided, directory-sync downgrades to a low-frequency
  // fallback and SSE events drive targeted per-workspace sync. Other tasks
  // (pair-cleanup, grant-healing, share-retry) always run on intervals.
  useEffect(() => {
    const appviewUrl = import.meta.env.VITE_APPVIEW_URL as string | undefined;
    // eslint-disable-next-line functional/no-let -- handle assigned inside async IIFE
    let handle: ReturnType<typeof startDaemon> | null = null;
    void Opake.taskDefs().then((defs) => {
      handle = startDaemon(getOpake(), defs, taskStore, {
        sse: appviewUrl ? {
          appviewUrl,
          onRecordChanged: () => reloadCurrentDirectory(),
        } : undefined,
        onWorkspaceUpdated: () => {
          reloadCurrentDirectory();
          void useWorkspaceStore.getState().loadWorkspaces();
        },
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
