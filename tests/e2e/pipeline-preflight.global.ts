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
// spec:e2e-testing § Reactive tiers verify pipeline liveness before running

import { runDevenvPipelinePreflight } from "../helpers/pipeline-probe-devenv.js";

export default async function globalSetup(): Promise<void> {
  await runDevenvPipelinePreflight("web e2e");
}
