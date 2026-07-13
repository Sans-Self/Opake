// Vitest globalSetup: the CLI federation tier's pipeline preflight, run once at
// suite entry before any federation spec. A stalled PDS → firehose → indexer
// pipeline fails the run here, in seconds and attributed, instead of every
// SSE-echo spec burning its multi-minute timeout into a misattributed red.
//
// Registered globally in vitest.config.ts, so it fires for every `vitest run`
// — including the default fake-pds CLI tier, which has no dev-env and no
// reactive specs. Gate on OPAKE_TEST_ENV=devenv (set only by
// `just e2e-federation`) and no-op otherwise.
//
// spec:e2e-testing § Reactive tiers verify pipeline liveness before running

import { runDevenvPipelinePreflight } from "./pipeline-probe-devenv.js";

export default async function setup(): Promise<void> {
  if (process.env.OPAKE_TEST_ENV !== "devenv") return;
  await runDevenvPipelinePreflight("CLI federation");
}
