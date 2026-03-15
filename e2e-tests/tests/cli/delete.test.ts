// §4 from AGENT-BLACKBOX-TEST.md: Delete

import { describe, it, expect } from "vitest";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { useFixture } from "../../helpers/fixture.js";

const fx = useFixture();

describe("delete", () => {
  it("rm -y deletes a file", async () => {
    const testFile = join(fx.workDir, "doomed.txt");
    writeFileSync(testFile, "delete me");
    await fx.opake(["upload", testFile]);

    const rm = await fx.opake(["rm", "doomed.txt", "-y"]);
    expect(rm.code).toBe(0);

    const ls = await fx.opake(["ls"]);
    expect(ls.stdout).not.toContain("doomed.txt");
  });

  it("rm by path deletes from directory", async () => {
    await fx.opake(["mkdir", "RmDir"]);

    const testFile = join(fx.workDir, "nested.txt");
    writeFileSync(testFile, "nested content");
    await fx.opake(["upload", testFile, "--dir", "RmDir"]);

    const rm = await fx.opake(["rm", "RmDir/nested.txt", "-y"]);
    expect(rm.code).toBe(0);
  });

  it("rm empty directory succeeds", async () => {
    await fx.opake(["mkdir", "EmptyDir"]);
    const rm = await fx.opake(["rm", "EmptyDir", "-y"]);
    expect(rm.code).toBe(0);
  });

  it("rm non-empty directory without -r fails", async () => {
    await fx.opake(["mkdir", "FullDir"]);

    const testFile = join(fx.workDir, "child.txt");
    writeFileSync(testFile, "child");
    await fx.opake(["upload", testFile, "--dir", "FullDir"]);

    const rm = await fx.opake(["rm", "FullDir", "-y"]);
    expect(rm.code).not.toBe(0);
    expect(rm.stderr).toContain("not empty");
  });

  it("rm -r recursively deletes directory", async () => {
    await fx.opake(["mkdir", "RecDir"]);

    const testFile = join(fx.workDir, "rec-child.txt");
    writeFileSync(testFile, "recursive child");
    await fx.opake(["upload", testFile, "--dir", "RecDir"]);

    const rm = await fx.opake(["rm", "-r", "RecDir", "-y"]);
    expect(rm.code).toBe(0);

    const tree = await fx.opake(["tree"]);
    expect(tree.stdout).not.toContain("RecDir");
  });
});
