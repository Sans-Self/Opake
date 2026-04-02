// §5b from AGENT-BLACKBOX-TEST.md: Metadata Management

import { describe, it, expect } from "vitest";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { useFixture } from "../../helpers/fixture.js";

const fx = useFixture();

describe("metadata", () => {
  it("show displays name and mime type", async () => {
    const testFile = join(fx.workDir, "meta-target.txt");
    writeFileSync(testFile, "metadata test content");
    await fx.opake(["upload", testFile]);

    const show = await fx.opake(["metadata", "show", "meta-target.txt"]);
    expect(show.code).toBe(0);
    expect(show.stdout).toContain("meta-target.txt");
    expect(show.stdout).toContain("text/plain");
  });

  it("rename changes the document name", async () => {
    const testFile = join(fx.workDir, "rename-me.txt");
    writeFileSync(testFile, "will be renamed");
    await fx.opake(["upload", testFile]);

    const rename = await fx.opake(["metadata", "rename", "rename-me.txt", "renamed.txt"]);
    expect(rename.code).toBe(0);
    expect(rename.stdout).toContain("renamed.txt");

    const ls = await fx.opake(["ls"]);
    expect(ls.stdout).toContain("renamed.txt");
    expect(ls.stdout).not.toContain("rename-me.txt");
  });

  it("describe sets a description", async () => {
    const testFile = join(fx.workDir, "desc-target.txt");
    writeFileSync(testFile, "description test");
    await fx.opake(["upload", testFile]);

    const desc = await fx.opake(["metadata", "describe", "desc-target.txt", "Annual tax return"]);
    expect(desc.code).toBe(0);

    const show = await fx.opake(["metadata", "show", "desc-target.txt"]);
    expect(show.stdout).toContain("Annual tax return");
  });

  it("describe --clear removes description", async () => {
    const testFile = join(fx.workDir, "clear-desc.txt");
    writeFileSync(testFile, "clear test");
    await fx.opake(["upload", testFile]);

    await fx.opake(["metadata", "describe", "clear-desc.txt", "temporary"]);
    const clear = await fx.opake(["metadata", "describe", "clear-desc.txt", "--clear"]);
    expect(clear.code).toBe(0);

    const show = await fx.opake(["metadata", "show", "clear-desc.txt"]);
    expect(show.stdout).not.toContain("temporary");
  });

  it("tag add and remove", async () => {
    const testFile = join(fx.workDir, "tag-target.txt");
    writeFileSync(testFile, "tag test");
    await fx.opake(["upload", testFile]);

    const add = await fx.opake(["metadata", "tag", "add", "tag-target.txt", "finance"]);
    expect(add.code).toBe(0);
    expect(add.stdout).toContain("finance");

    const add2 = await fx.opake(["metadata", "tag", "add", "tag-target.txt", "2026"]);
    expect(add2.stdout).toContain("finance");
    expect(add2.stdout).toContain("2026");

    const remove = await fx.opake(["metadata", "tag", "remove", "tag-target.txt", "finance"]);
    expect(remove.code).toBe(0);
    expect(remove.stdout).toContain("2026");
    expect(remove.stdout).not.toContain("finance");
  });

  it("show nonexistent file fails", async () => {
    const show = await fx.opake(["metadata", "show", "no-such-file.txt"]);
    expect(show.code).not.toBe(0);
  });

  it("rename nonexistent file fails", async () => {
    const rename = await fx.opake(["metadata", "rename", "ghost.txt", "new.txt"]);
    expect(rename.code).not.toBe(0);
  });

  it("describe without text or --clear fails", async () => {
    const testFile = join(fx.workDir, "no-desc-arg.txt");
    writeFileSync(testFile, "needs argument");
    await fx.opake(["upload", testFile]);

    const desc = await fx.opake(["metadata", "describe", "no-desc-arg.txt"]);
    expect(desc.code).not.toBe(0);
  });

  it("duplicate tag is idempotent", async () => {
    const testFile = join(fx.workDir, "dupe-tag.txt");
    writeFileSync(testFile, "dupe test");
    await fx.opake(["upload", testFile]);

    await fx.opake(["metadata", "tag", "add", "dupe-tag.txt", "unique-tag"]);
    await fx.opake(["metadata", "tag", "add", "dupe-tag.txt", "unique-tag"]);

    const show = await fx.opake(["metadata", "show", "dupe-tag.txt"]);
    const matches = show.stdout.match(/unique-tag/g);
    expect(matches?.length).toBe(1);
  });
});
