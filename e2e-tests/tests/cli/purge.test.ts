// Purge — destructive delete of all Opake data

import { describe, it, expect } from "vitest";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { useFixture } from "../../helpers/fixture.js";

const fx = useFixture();

describe("purge", () => {
  it("purge --dry-run previews without deleting", async () => {
    const testFile = join(fx.workDir, "dry-run.txt");
    writeFileSync(testFile, "should survive");
    await fx.opake(["upload", testFile]);

    const purge = await fx.opake(["purge", "--dry-run"]);
    expect(purge.code).toBe(0);
    expect(purge.stdout).toContain("would be deleted");

    const ls = await fx.opake(["ls"]);
    expect(ls.stdout).toContain("dry-run.txt");
  });

  it("purge --force deletes all opake data", async () => {
    const testFile = join(fx.workDir, "purge-target.txt");
    writeFileSync(testFile, "will be purged");
    await fx.opake(["upload", testFile]);
    await fx.opake(["mkdir", "PurgeDir"]);

    const purge = await fx.opake(["purge", "--force"]);
    expect(purge.code).toBe(0);

    const ls = await fx.opake(["ls"]);
    expect(ls.stdout.trim()).toBe("");
  });
});
