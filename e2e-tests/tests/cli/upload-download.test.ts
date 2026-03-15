// §2 from AGENT-BLACKBOX-TEST.md: Upload and Download (Direct Encryption)

import { describe, it, expect } from "vitest";
import { writeFileSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { useFixture } from "../../helpers/fixture.js";

const fx = useFixture();

describe("upload and download", () => {
  it("upload → ls → download roundtrip", async () => {
    const testFile = join(fx.workDir, "hello.txt");
    writeFileSync(testFile, "hello opake");

    const upload = await fx.opake(["upload", testFile]);
    expect(upload.code).toBe(0);
    expect(upload.stdout).toContain("hello.txt");
    expect(upload.stdout).toContain("at://");

    const ls = await fx.opake(["ls"]);
    expect(ls.code).toBe(0);
    expect(ls.stdout).toContain("hello.txt");

    const downloadPath = join(fx.workDir, "downloaded.txt");
    const download = await fx.opake(["download", "hello.txt", "-o", downloadPath]);
    expect(download.code).toBe(0);
    expect(readFileSync(downloadPath, "utf-8")).toBe("hello opake");
  });

  it("ls -l shows mime type and size", async () => {
    const testFile = join(fx.workDir, "doc.txt");
    writeFileSync(testFile, "some content");

    await fx.opake(["upload", testFile]);
    const ls = await fx.opake(["ls", "-l"]);

    expect(ls.code).toBe(0);
    expect(ls.stdout).toContain("doc.txt");
    expect(ls.stdout).toContain("text/plain");
  });

  it("download by AT-URI", async () => {
    const testFile = join(fx.workDir, "uri-test.txt");
    writeFileSync(testFile, "uri content");

    const upload = await fx.opake(["upload", testFile]);
    const uriMatch = upload.stdout.match(/at:\/\/[^\s]+/);
    expect(uriMatch).toBeTruthy();

    const downloadPath = join(fx.workDir, "uri-download.txt");
    const download = await fx.opake(["download", uriMatch![0]!, "-o", downloadPath]);
    expect(download.code).toBe(0);
    expect(readFileSync(downloadPath, "utf-8")).toBe("uri content");
  });

  it("download refuses to overwrite existing file", async () => {
    const testFile = join(fx.workDir, "overwrite.txt");
    writeFileSync(testFile, "original");
    await fx.opake(["upload", testFile]);

    const outputPath = join(fx.workDir, "already-exists.txt");
    writeFileSync(outputPath, "existing");

    const download = await fx.opake(["download", "overwrite.txt", "-o", outputPath]);
    expect(download.code).not.toBe(0);
    expect(download.stderr).toContain("exists");
  });

  it("upload and download empty file", async () => {
    const testFile = join(fx.workDir, "empty.bin");
    writeFileSync(testFile, "");

    const upload = await fx.opake(["upload", testFile]);
    expect(upload.code).toBe(0);

    const downloadPath = join(fx.workDir, "empty-download.bin");
    const download = await fx.opake(["download", "empty.bin", "-o", downloadPath]);
    expect(download.code).toBe(0);
    expect(readFileSync(downloadPath, "utf-8")).toBe("");
  });

  it("cat decrypts to stdout", async () => {
    const testFile = join(fx.workDir, "cat-test.txt");
    writeFileSync(testFile, "stdout content");

    await fx.opake(["upload", testFile]);
    const cat = await fx.opake(["cat", "cat-test.txt"]);

    expect(cat.code).toBe(0);
    expect(cat.stdout).toBe("stdout content");
  });
});
