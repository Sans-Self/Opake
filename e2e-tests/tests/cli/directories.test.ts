// §3 from AGENT-BLACKBOX-TEST.md: Directories

import { describe, it, expect } from "vitest";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { useFixture } from "../../helpers/fixture.js";

const fx = useFixture();

describe("directories", () => {
  it("mkdir creates a directory", async () => {
    const result = await fx.opake(["mkdir", "Photos"]);
    expect(result.code).toBe(0);
    expect(result.stdout).toContain("Photos");
  });

  it("upload into a directory", async () => {
    await fx.opake(["mkdir", "Docs"]);

    const testFile = join(fx.workDir, "notes.txt");
    writeFileSync(testFile, "meeting notes");

    const upload = await fx.opake(["upload", testFile, "--dir", "Docs"]);
    expect(upload.code).toBe(0);
  });

  it("tree shows directory structure", async () => {
    await fx.opake(["mkdir", "TreeTest"]);

    const testFile = join(fx.workDir, "leaf.txt");
    writeFileSync(testFile, "leaf content");
    await fx.opake(["upload", testFile, "--dir", "TreeTest"]);

    const tree = await fx.opake(["tree"]);
    expect(tree.code).toBe(0);
    expect(tree.stdout).toContain("TreeTest");
    expect(tree.stdout).toContain("leaf.txt");
  });

  it("cat by path", async () => {
    await fx.opake(["mkdir", "CatDir"]);

    const testFile = join(fx.workDir, "catfile.txt");
    writeFileSync(testFile, "cat path content");
    await fx.opake(["upload", testFile, "--dir", "CatDir"]);

    const cat = await fx.opake(["cat", "CatDir/catfile.txt"]);
    expect(cat.code).toBe(0);
    expect(cat.stdout).toBe("cat path content");
  });

  it("move file into directory", async () => {
    await fx.opake(["mkdir", "MoveTarget"]);

    const testFile = join(fx.workDir, "movable.txt");
    writeFileSync(testFile, "will be moved");
    await fx.opake(["upload", testFile]);

    const mv = await fx.opake(["move", "movable.txt", "MoveTarget/"]);
    expect(mv.code).toBe(0);

    const tree = await fx.opake(["tree"]);
    expect(tree.stdout).toContain("MoveTarget");
    expect(tree.stdout).toContain("movable.txt");
  });
});
