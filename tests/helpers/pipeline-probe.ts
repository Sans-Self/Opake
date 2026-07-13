// Pipeline-liveness preflight for the reactive e2e tiers (web e2e, CLI
// federation). One real record is written through a fixture actor's session
// and observed to arrive at the indexer over the full PDS → firehose → indexer
// path; a dead pipeline fails the run in seconds, attributed, before any spec
// burns its multi-minute SSE-echo timeout. The two SSE-dependent tiers would
// otherwise turn a pipeline stall into 10+ minutes of misattributed red.
//
// Transport is injected: both tiers drive the in-network CLI probe (see
// pipeline-probe-devenv.ts), the only path that reaches the dev-env indexer
// from the host without a browser boot-cost floor that would swamp the ~1s
// pipeline latency this preflight exists to measure. This module is pure
// timing, attribution, and reporting — no docker, no browser, no clock of its
// own (both are injected so the negative path is unit-testable).
//
// spec:e2e-testing § Reactive tiers verify pipeline liveness before running

/**
 * Degradation threshold. Above this the pipeline is LIVE but slow: the run
 * passes with a prominent warning naming the latency, and the suite proceeds.
 * A healthy dev-env delivers in ~1s, so a slow pass is signal, not noise.
 */
export const PIPELINE_DEGRADED_MS = 5_000;

/**
 * Hard window. Past this the pipeline is treated as dead and the run fails.
 * Deliberately far above healthy-path latency: tightening it toward ~1s would
 * fail warm-cache-but-healthy runs and reintroduce the very flakiness the
 * probe exists to eliminate. Revisit against the indexer's lag telemetry, same
 * policy as the retry constants — never toward the healthy path.
 */
export const PIPELINE_WINDOW_MS = 30_000;

/**
 * Which half of the pipeline the failure is attributed to. `write` is a
 * fixture/auth problem (the probe never reached the pipeline); `arrival` is a
 * genuine pipeline stall (the write landed but never propagated). Both fail
 * the run, each named plainly for what it is — a write failure must never
 * masquerade as a pipeline failure.
 */
export type ProbeAttribution = "write" | "arrival";

export class PipelineProbeError extends Error {
  constructor(
    readonly attribution: ProbeAttribution,
    message: string,
  ) {
    super(message);
    this.name = "PipelineProbeError";
  }
}

/** Transport-specific probe steps. Injected per tier. */
export interface ProbeHooks {
  /** Human label for the tier, e.g. "web e2e" or "CLI federation". */
  readonly tier: string;
  /**
   * Write the ephemeral probe record through a real session and resolve once
   * the write has committed. Throwing here attributes the failure to `write`
   * (fixture/auth), never to the pipeline. Returns a label naming the record
   * (its AT-URI or filename) for the diagnosis.
   */
  write(): Promise<{ readonly label: string }>;
  /** Poll the indexer once; resolve true when the probe record is observed. */
  hasArrived(): Promise<boolean>;
  /** Best-effort delete of the probe record (probe hygiene). */
  cleanup?(): Promise<void>;
  /** Best-effort pipeline evidence for the failure message (cursor/lag). */
  evidence?(): Promise<string>;
}

/** Injectable clock, logging, and thresholds. Defaults are the real ones. */
export interface ProbeDeps {
  now?: () => number;
  sleep?: (ms: number) => Promise<void>;
  log?: (message: string) => void;
  warn?: (message: string) => void;
  pollIntervalMs?: number;
  degradedMs?: number;
  windowMs?: number;
}

export interface ProbeOutcome {
  readonly tier: string;
  readonly label: string;
  readonly latencyMs: number;
  readonly degraded: boolean;
}

const defaultSleep = (ms: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Run the preflight: write, poll for arrival within the window, report the
 * measured write-to-arrival latency on every outcome. Resolves with the
 * outcome on a live pipeline (warning first if degraded); throws a
 * `PipelineProbeError` — attributed to `write` or `arrival` — otherwise.
 */
export async function runPipelineProbe(
  hooks: ProbeHooks,
  deps: ProbeDeps = {},
): Promise<ProbeOutcome> {
  const now = deps.now ?? (() => Date.now());
  const sleep = deps.sleep ?? defaultSleep;
  const log = deps.log ?? ((message: string) => console.log(message));
  const warn = deps.warn ?? ((message: string) => console.warn(message));
  const interval = deps.pollIntervalMs ?? 1_000;
  const degradedMs = deps.degradedMs ?? PIPELINE_DEGRADED_MS;
  const windowMs = deps.windowMs ?? PIPELINE_WINDOW_MS;
  const { tier } = hooks;

  // eslint-disable-next-line functional/no-let
  let written: { readonly label: string };
  try {
    written = await hooks.write();
  } catch (error) {
    throw new PipelineProbeError(
      "write",
      `[pipeline-preflight:${tier}] probe WRITE failed before it could reach the pipeline — a ` +
        `fixture/auth problem, not a pipeline failure and not a test failure. A stale auth snapshot ` +
        `(401 "exp claim" / "Invalid refresh token") produces exactly this; refresh with E2E_REAUTH=1 ` +
        `or rebuild the dev-env fixtures. Cause: ${describeError(error)}`,
    );
  }

  // Write-to-arrival is measured from write completion, isolating the pipeline
  // (PDS → firehose → indexer) lag — the band that actually stalls — from
  // client-side crypto/upload time. The window bounds the same interval.
  const writtenAt = now();
  for (;;) {
    // eslint-disable-next-line functional/no-let
    let arrived = false;
    try {
      arrived = await hooks.hasArrived();
    } catch {
      arrived = false; // a transient poll error is not arrival; keep polling until the window
    }
    if (arrived) break;

    if (now() - writtenAt >= windowMs) {
      const evidence = hooks.evidence
        ? await hooks.evidence().catch(() => "unavailable")
        : "not gathered";
      await safeCleanup(hooks);
      throw new PipelineProbeError(
        "arrival",
        `[pipeline-preflight:${tier}] pipeline did not deliver within ${windowMs / 1000}s — ` +
          `infrastructure failure, not a test failure; see the jetstream↔relay socket issue. Zero ` +
          `specs run. Probe write: ${written.label}; written at ${new Date(writtenAt).toISOString()}; ` +
          `window ${windowMs / 1000}s. Pipeline evidence: ${evidence}`,
      );
    }
    await sleep(interval);
  }

  const latencyMs = now() - writtenAt;
  await safeCleanup(hooks);

  const degraded = latencyMs >= degradedMs;
  if (degraded) {
    warn(
      `⚠️  [pipeline-preflight:${tier}] pipeline is LIVE but SLOW — probe delivered in ${latencyMs}ms ` +
        `(degradation threshold ${degradedMs}ms; hard window ${windowMs}ms). A healthy dev-env ` +
        `delivers in ~1s; treat this as a trend signal, not noise. Suite proceeds.`,
    );
  } else {
    log(
      `✓ [pipeline-preflight:${tier}] pipeline live — probe delivered in ${latencyMs}ms ` +
        `(< ${degradedMs}ms degradation threshold). Suite proceeds.`,
    );
  }

  return { tier, label: written.label, latencyMs, degraded };
}

async function safeCleanup(hooks: ProbeHooks): Promise<void> {
  if (!hooks.cleanup) return;
  try {
    await hooks.cleanup();
  } catch {
    /* probe hygiene only — the fixture reset tolerates residue */
  }
}

function describeError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
