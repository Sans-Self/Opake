// Account config (PDS-synced preferences)

import { describe, it, expect } from "vitest";
import { useFixture } from "../../helpers/fixture.js";

const fx = useFixture();

describe("opake config", () => {
  it("shows defaults when no config record exists", async () => {
    const result = await fx.opake(["config"]);
    expect(result.code).toBe(0);
    expect(result.stdout).toContain("telemetry");
    expect(result.stdout).toContain("disabled");
  });

  it("sets telemetry-enabled", async () => {
    const set = await fx.opake(["config", "set", "telemetry-enabled", "true"]);
    expect(set.code).toBe(0);
    expect(set.stdout).toContain("enabled");

    const show = await fx.opake(["config"]);
    expect(show.stdout).toContain("enabled");
  });

  it("sets appview-url", async () => {
    const set = await fx.opake(["config", "set", "appview-url", "https://appview.test"]);
    expect(set.code).toBe(0);
    expect(set.stdout).toContain("appview.test");

    const show = await fx.opake(["config"]);
    expect(show.stdout).toContain("appview.test");
  });

  it("rejects unknown config key", async () => {
    const result = await fx.opake(["config", "set", "nonexistent-key", "value"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("unknown config key");
    expect(result.stderr).toContain("telemetry-enabled");
  });

  it("clears appview-url with empty string", async () => {
    await fx.opake(["config", "set", "appview-url", "https://will-be-cleared.test"]);
    const clear = await fx.opake(["config", "set", "appview-url", ""]);
    expect(clear.code).toBe(0);
    expect(clear.stdout).toContain("(not set)");
  });
});
