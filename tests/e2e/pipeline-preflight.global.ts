// Playwright globalSetup: the web e2e tier's pipeline preflight, run once
// before any project — including the OAuth auth setup — so a stalled
// PDS → firehose → indexer pipeline fails the run in seconds, attributed,
// rather than after every SSE-echo spec burns its multi-minute timeout. When
// this throws, Playwright aborts the whole run: zero specs execute.
//
// The probe drives the in-network CLI (the only path to the dev-env indexer
// from the host); it does not need the auth storageState, which is why it runs
// before setup. Web auth remains proven by the setup project the specs depend
// on. See pipeline-probe-devenv.ts for why the CLI transport is correct here.
//
// A namespaced run provisions its actors here too, ahead of the probe: the probe
// writes a record as a fixture actor, so under E2E_ACTOR_NS that actor has to
// exist before it can prove anything. Provisioning is idempotent, so only the
// first run in a namespace pays — and doing it in globalSetup (once, before any
// worker) is also what keeps parallel setup workers from racing each other into
// duplicate account creation. The probe then writes its records as the
// namespace's own actor, leaving the default population untouched.
//
// spec:e2e-testing § Reactive tiers verify pipeline liveness before running

import { actorNamespace } from "./namespace.js";
import { provisionNamespace } from "./pds-admin.js";
import { runDevenvPipelinePreflight } from "../helpers/pipeline-probe-devenv.js";

export default async function globalSetup(): Promise<void> {
  const ns = actorNamespace();
  if (ns !== "") {
    const provisioned = await provisionNamespace(ns);
    console.log(
      provisioned.length > 0
        ? `[auth.setup] provisioned namespace ${ns}: ${provisioned.join(", ")}`
        : `[auth.setup] namespace ${ns} already provisioned`,
    );
  }
  await runDevenvPipelinePreflight("web e2e");
}
