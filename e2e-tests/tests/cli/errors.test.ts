// §7 from AGENT-BLACKBOX-TEST.md: Error Cases

import { describe, it, expect } from "vitest";
import { useFixture } from "../../helpers/fixture.js";

const fx = useFixture();

describe("error cases", () => {
  it("download nonexistent file errors", async () => {
    const result = await fx.opake(["download", "nonexistent-file.txt"]);
    expect(result.code).not.toBe(0);
  });

  it("download invalid AT-URI errors", async () => {
    const result = await fx.opake(["download", "not-a-uri"]);
    expect(result.code).not.toBe(0);
  });

  it("rm nonexistent file errors", async () => {
    const result = await fx.opake(["rm", "ghost-file.txt", "-y"]);
    expect(result.code).not.toBe(0);
  });

  it("resolve unknown handle errors", async () => {
    const result = await fx.opake(["resolve", "nobody.nonexistent"]);
    expect(result.code).not.toBe(0);
  });

  it("config set with invalid bool errors", async () => {
    const result = await fx.opake(["config", "set", "telemetry-enabled", "maybe"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("expected a boolean");
  });
});
