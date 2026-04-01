// Opake WASM worker — single worker for the entire app.
//
// All domain operations go through core via OpakeContext / FileManager.
// No standalone crypto, no direct PDS calls, no session threading.
//
// Pre-auth functions (DPoP, identity gen, DID resolution) are standalone
// because they run before an OpakeContext can be constructed.

import * as Comlink from "comlink";
import init, { bindingCheck, daemonTaskDefs, schemaVersion } from "@/wasm/opake-wasm/opake";
import wasmUrl from "@/wasm/opake-wasm/opake_bg.wasm?url";
import { setAccount, clearAccount } from "./context";
import { identityApi } from "./api/identity";
import { cabinetApi } from "./api/cabinet";
import { workspaceApi } from "./api/workspace";

console.debug("[worker] initializing WASM from", wasmUrl);
await init();
console.debug("[worker] ready, binding check:", bindingCheck());

// Start daemon after a short delay — gives setAccount time to set the DID.
// The daemon's leader lock + account check handle the case where no account is active.
import("./daemon")
  .then(({ startDaemon }) => {
    setTimeout(() => {
      console.info("[worker] starting daemon");
      startDaemon();
    }, 3000);
  })
  .catch(console.error);

const opakeApi = {
  ping(): string {
    return "pong";
  },

  bindingCheck(): string {
    return bindingCheck();
  },

  schemaVersion(): number {
    return schemaVersion();
  },

  daemonTaskDefs,

  // Lifecycle — tell the worker which account to use
  setAccount: (did: string) => {
    console.info("[worker] setAccount called with", did);
    setAccount(did);
  },
  clearAccount,

  // Pre-auth (no OpakeContext needed)
  ...identityApi,

  // Domain (all through core)
  ...cabinetApi,
  ...workspaceApi,
};

export type OpakeApi = typeof opakeApi;

Comlink.expose(opakeApi);
