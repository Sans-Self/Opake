// Namespace teardown entry point (`just e2e-ns-clean <ns>`): delete a
// namespace's accounts — and with them their records and blobs — from the
// dev-env, then drop the local artifacts that belonged to that run.
//
// The namespace's actors are addressed by derived handle, so nothing had to be
// remembered between provisioning and teardown: re-deriving the six handles
// from the namespace name is the whole registry.
import { rmSync } from "node:fs";
import { deprovisionNamespace } from "./pds-admin";
import { assertNamespace, nsPaths } from "./namespace";

const ns = process.argv[2]?.trim() ?? "";
if (ns === "") {
  console.error(
    "usage: bun run e2e/ns-clean.ts <namespace>\n" +
      "refusing to run without one — the default population is checked in and is " +
      "cleared only by `just dev-env-reset`",
  );
  process.exit(2);
}
assertNamespace(ns);

const deleted = await deprovisionNamespace(ns);
console.log(
  deleted.length > 0
    ? `deprovisioned ${ns}: ${deleted.join(", ")}`
    : `namespace ${ns} had no live accounts`,
);

const { authDir, outputDir, reportDir } = nsPaths(ns);
for (const dir of [authDir, outputDir, reportDir]) {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`removed artifacts for ${ns}`);
