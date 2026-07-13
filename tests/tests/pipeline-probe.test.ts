// Unit coverage for the pipeline preflight's timeout / attribution / latency
// logic (pipeline-probe.ts). The transport hooks and the clock are injected,
// so the negative paths — write failure vs pipeline stall — and the
// degradation warning are exercised deterministically, with no dev-env, no
// docker, and no real waiting. The positive path (a real write propagating end
// to end) is verified by construction on every real tier run.
//
// spec:e2e-testing § Reactive tiers verify pipeline liveness before running

import { describe, it, expect, vi } from "vitest";
import {
  PIPELINE_DEGRADED_MS,
  PIPELINE_WINDOW_MS,
  PipelineProbeError,
  runPipelineProbe,
  type ProbeDeps,
  type ProbeHooks,
} from "../helpers/pipeline-probe.js";

/** A fake clock whose only advance is the probe's own `sleep`, so `now()` is a
 *  pure function of how many poll intervals have elapsed. */
function fakeClock(): Pick<ProbeDeps, "now" | "sleep"> {
  // eslint-disable-next-line functional/no-let
  let t = 0;
  return {
    now: () => t,
    sleep: async (ms: number) => {
      t += ms;
    },
  };
}

/** Capture the human-facing output so assertions can read it back. */
function captureOutput(): {
  deps: Pick<ProbeDeps, "log" | "warn">;
  logs: string[];
  warns: string[];
} {
  const logs: string[] = [];
  const warns: string[] = [];
  return {
    logs,
    warns,
    deps: {
      log: (m: string) => logs.push(m),
      warn: (m: string) => warns.push(m),
    },
  };
}

const INTERVAL = 1_000;

/** Hooks that arrive after `arriveAfterPolls` unsuccessful polls (0 = at once). */
function arrivingHooks(
  arriveAfterPolls: number,
  overrides: Partial<ProbeHooks> = {},
): { hooks: ProbeHooks; cleanup: ReturnType<typeof vi.fn> } {
  const cleanup = vi.fn(async () => {});
  // eslint-disable-next-line functional/no-let
  let polls = 0;
  const hooks: ProbeHooks = {
    tier: "unit",
    write: async () => ({ label: "at://did:plc:probe/app.opake.document/xyz" }),
    hasArrived: async () => polls++ >= arriveAfterPolls,
    cleanup,
    evidence: async () => "cursor_age_secs=412 last_event_age_ms=none",
    ...overrides,
  };
  return { hooks, cleanup };
}

describe("runPipelineProbe", () => {
  it("returns a healthy outcome and reports latency when the write arrives promptly", async () => {
    const { hooks, cleanup } = arrivingHooks(0);
    const out = captureOutput();

    const result = await runPipelineProbe(hooks, {
      ...fakeClock(),
      ...out.deps,
      pollIntervalMs: INTERVAL,
    });

    expect(result.latencyMs).toBe(0);
    expect(result.degraded).toBe(false);
    expect(cleanup).toHaveBeenCalledTimes(1);
    expect(out.warns).toHaveLength(0);
    expect(out.logs.join("\n")).toContain("pipeline live");
    expect(out.logs.join("\n")).toContain("0ms");
  });

  it("passes with a prominent warning naming the latency when degraded but within the window", async () => {
    // Arrive one interval past the degradation threshold, still under the window.
    const pollsToDegrade = PIPELINE_DEGRADED_MS / INTERVAL + 1;
    const { hooks } = arrivingHooks(pollsToDegrade);
    const out = captureOutput();

    const result = await runPipelineProbe(hooks, {
      ...fakeClock(),
      ...out.deps,
      pollIntervalMs: INTERVAL,
    });

    expect(result.degraded).toBe(true);
    expect(result.latencyMs).toBe(pollsToDegrade * INTERVAL);
    expect(result.latencyMs).toBeLessThan(PIPELINE_WINDOW_MS);
    expect(out.logs).toHaveLength(0);
    expect(out.warns).toHaveLength(1);
    expect(out.warns[0]).toContain("LIVE but SLOW");
    expect(out.warns[0]).toContain(`${result.latencyMs}ms`);
  });

  it("fails attributed to the pipeline, naming evidence and window, when the write never arrives", async () => {
    const { hooks, cleanup } = arrivingHooks(Number.POSITIVE_INFINITY);
    const out = captureOutput();

    const error = await runPipelineProbe(hooks, {
      ...fakeClock(),
      ...out.deps,
      pollIntervalMs: INTERVAL,
    }).catch((e: unknown) => e);

    expect(error).toBeInstanceOf(PipelineProbeError);
    const probeError = error as PipelineProbeError;
    expect(probeError.attribution).toBe("arrival");
    expect(probeError.message).toContain("infrastructure failure");
    expect(probeError.message).toContain("30s");
    expect(probeError.message).toContain("cursor_age_secs=412"); // evidence surfaced
    expect(probeError.message).toContain("at://did:plc:probe"); // probe record named
    // Best-effort cleanup still runs on the failure path.
    expect(cleanup).toHaveBeenCalledTimes(1);
  });

  it("attributes a failed write to fixture/auth, distinct from a pipeline failure", async () => {
    const { hooks } = arrivingHooks(0, {
      write: async () => {
        throw new Error('401: "Invalid refresh token"');
      },
    });
    const out = captureOutput();

    const error = await runPipelineProbe(hooks, {
      ...fakeClock(),
      ...out.deps,
      pollIntervalMs: INTERVAL,
    }).catch((e: unknown) => e);

    expect(error).toBeInstanceOf(PipelineProbeError);
    const probeError = error as PipelineProbeError;
    expect(probeError.attribution).toBe("write");
    expect(probeError.message).toContain("not a pipeline failure");
    expect(probeError.message).toContain("E2E_REAUTH=1");
    expect(probeError.message).toContain("Invalid refresh token"); // cause preserved
  });

  it("treats a transient poll error as not-yet-arrived rather than a failure", async () => {
    const cleanup = vi.fn(async () => {});
    // eslint-disable-next-line functional/no-let
    let polls = 0;
    const hooks: ProbeHooks = {
      tier: "unit",
      write: async () => ({ label: "probe" }),
      hasArrived: async () => {
        polls += 1;
        if (polls < 3) throw new Error("indexer briefly unreachable");
        return polls >= 4; // one clean false, then arrive
      },
      cleanup,
      evidence: async () => "n/a",
    };
    const out = captureOutput();

    const result = await runPipelineProbe(hooks, {
      ...fakeClock(),
      ...out.deps,
      pollIntervalMs: INTERVAL,
    });

    expect(result.degraded).toBe(false);
    expect(result.latencyMs).toBe(3 * INTERVAL); // 3 non-arrivals (2 throwing, 1 false), then true
  });
});
