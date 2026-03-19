// Opake WASM worker — single worker for the entire app.
//
// Composes focused API modules into one Comlink-exposed surface:
//   crypto    — encrypt, decrypt, wrap, unwrap
//   identity  — keypairs, seed phrases, DPoP, DID resolution
//   tree      — stateful directory tree handle
//   pds       — PDS operations via WasmTransport

import * as Comlink from "comlink";
import init, { bindingCheck, daemonTaskDefs } from "@/wasm/opake-wasm/opake";
import { cryptoApi } from "./api/crypto";
import { identityApi } from "./api/identity";
import { treeApi } from "./api/tree";
import { pdsApi } from "./api/pds";

console.debug("[worker] initializing WASM");
await init();
console.debug("[worker] ready, binding check:", bindingCheck());

const opakeApi = {
  ping(): string {
    return "pong";
  },

  bindingCheck(): string {
    return bindingCheck();
  },

  daemonTaskDefs,

  ...cryptoApi,
  ...identityApi,
  ...treeApi,
  ...pdsApi,
};

export type OpakeApi = typeof opakeApi;

Comlink.expose(opakeApi);
