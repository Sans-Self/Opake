// In-network CLI transport for the pipeline preflight (pipeline-probe.ts),
// shared verbatim by both reactive tiers. The probe writes one ephemeral
// cabinet document through a fixture actor's real CLI session, polls the
// indexer-backed `ls` (a direct authenticated indexer query — the CLI has no
// keepers, so `ls` reflects exactly what the indexer resolved, with no
// optimistic overlay to contaminate the arrival measurement), and deletes it.
//
// Why the CLI even for the web tier: the dev-env indexer is reachable only
// from inside the compose network (Caddy publishes it browser-side as
// indexer.test, unresolvable from the host). A browser-session probe would
// carry an unavoidable WASM-boot cost that swamps the ~1s pipeline latency the
// amended spec requires the probe to measure. The pipeline being probed —
// PDS → firehose → indexer — is the same infrastructure every web spec's
// SSE-echo depends on, and web auth is proven independently by the setup
// project the preflight gates. So a CLI write that propagates end-to-end
// verifies exactly the liveness the web specs need.
//
// spec:e2e-testing § Reactive tiers verify pipeline liveness before running

import {
  cli,
  indexerHealth,
  login,
  stackIsUp,
  startCli,
  stopCli,
  uploadTextToCabinet,
} from "./devenv.js";
import {
  runPipelineProbe,
  type ProbeDeps,
  type ProbeHooks,
  type ProbeOutcome,
} from "./pipeline-probe.js";

// A pooled, always-seeded actor. The preflight runs to completion before any
// spec starts, so there is no concurrent mutation to race; the probe document
// is uniquely named and lives at the cabinet root, where no spec asserts (specs
// scope to their own unique folders), and it is deleted after — worker
// isolation holds on both the no-shared-mutation and the no-residue fronts.
// spec:e2e-testing § Parallel workers do not share mutable state
const PROBE_ACTOR = "alice";

/**
 * Build the CLI-backed probe hooks for a tier. Assumes the shared CLI
 * container is up (`startCli`) and `PROBE_ACTOR` is logged in.
 */
export function devenvProbeHooks(tier: string): ProbeHooks {
  const filename = `preflight-${Date.now().toString(36)}-${Math.floor(
    Math.random() * 1e6,
  ).toString(36)}.txt`;
  // `upload` names the document after the staged file's basename, which
  // `uploadTextToCabinet` prefixes with the actor name.
  const docName = `${PROBE_ACTOR}-${filename}`;

  return {
    tier,
    async write() {
      const uri = await uploadTextToCabinet(
        PROBE_ACTOR,
        filename,
        `pipeline preflight probe — ${new Date().toISOString()}`,
      );
      return { label: uri };
    },
    async hasArrived() {
      const res = await cli(PROBE_ACTOR, ["ls"]);
      return res.code === 0 && res.stdout.includes(docName);
    },
    async cleanup() {
      await cli(PROBE_ACTOR, ["rm", docName, "-y"]);
    },
    evidence: indexerHealth,
  };
}

/**
 * Run the pipeline preflight for a reactive tier against the dev-env: spin the
 * shared CLI container, log the probe actor in, probe, and tear the container
 * down. Throws (aborting the run before any spec) if the stack is down or the
 * probe fails. `deps` is for tests; production callers pass none.
 */
export async function runDevenvPipelinePreflight(
  tier: string,
  deps?: ProbeDeps,
): Promise<ProbeOutcome> {
  if (!(await stackIsUp())) {
    throw new Error(
      `[pipeline-preflight:${tier}] dev-env stack is not up — run \`just dev-env-up\` before the ` +
        `reactive tiers. This is a setup problem, not a pipeline or test failure.`,
    );
  }
  await startCli();
  try {
    await login(PROBE_ACTOR);
    return await runPipelineProbe(devenvProbeHooks(tier), deps);
  } finally {
    await stopCli();
  }
}
