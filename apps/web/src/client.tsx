// Client entry — TanStack Start hydrates the app from here.

import { enableMapSet, enableArrayMethods } from "immer";
import { StrictMode, startTransition } from "react";
import { hydrateRoot } from "react-dom/client";
import { StartClient } from "@tanstack/react-start/client";
import { getOpakeWorker } from "@/lib/worker";
import { useAuthStore } from "@/stores/auth";
import { useWorkspaceStore } from "@/stores/workspaceBrowser";
import { expireSession } from "@/lib/authErrors";

enableMapSet();
enableArrayMethods();

console.debug("[opake] app starting");
getOpakeWorker(); // warm up WASM worker early
void useAuthStore.getState().boot(); // start session restore from IndexedDB

// Listen for daemon events via BroadcastChannel.
const daemonChannel = new BroadcastChannel("opake-daemon");
daemonChannel.addEventListener("message", (event: MessageEvent) => {
  const data = event.data as { type?: string; keyringUris?: string[] } | undefined;
  if (data?.type === "session-expired") {
    expireSession();
  }
  if (data?.type === "workspace-updated" && data.keyringUris) {
    const { activeKeyringUri, loadWorkspaceTree } = useWorkspaceStore.getState();
    console.debug(
      "[opake] workspace-updated broadcast:",
      data.keyringUris,
      "active:",
      activeKeyringUri,
    );
    if (activeKeyringUri && data.keyringUris.includes(activeKeyringUri)) {
      void loadWorkspaceTree(activeKeyringUri);
    }
  }
});

startTransition(() => {
  hydrateRoot(
    document,
    <StrictMode>
      <StartClient />
    </StrictMode>,
  );
});
