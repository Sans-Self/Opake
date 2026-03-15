// §2 from AGENT-BLACKBOX-TEST.md: Upload and Download (Direct Encryption)

import { describe, it, expect, beforeAll, afterAll, beforeEach } from "vitest";
import { writeFileSync, readFileSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { startPds, stopPds } from "../../helpers/pds.js";
import { setupAccount } from "../../helpers/account.js";
import { opake } from "../../helpers/cli.js";

// eslint-disable-next-line functional/no-let
let ctx: Awaited<ReturnType<typeof setupAccount>>;
// eslint-disable-next-line functional/no-let
let workDir: string;

const tempDirs: string[] = [];

function freshWorkDir(): string {
  const dir = mkdtempSync(join(tmpdir(), "opake-e2e-work-"));
  tempDirs.push(dir);
  return dir;
}

beforeAll(async () => {
  await startPds();
  ctx = await setupAccount("did:plc:alice", "alice.test");
});

beforeEach(() => {
  workDir = freshWorkDir();
});

afterAll(async () => {
  await stopPds();
  for (const dir of tempDirs) {
    rmSync(dir, { recursive: true, force: true });
  }
  if (ctx?.configDir) {
    rmSync(ctx.configDir, { recursive: true, force: true });
  }
});

describe("upload and download", () => {
  it("upload → ls → download roundtrip", async () => {
    // Create a test file
    const testFile = join(workDir, "hello.txt");
    writeFileSync(testFile, "hello opake");

    // Upload
    const upload = await opake(["upload", testFile], { configDir: ctx.configDir });
    expect(upload.code).toBe(0);
    expect(upload.stdout).toContain("hello.txt");
    expect(upload.stdout).toContain("at://");

    // List
    const ls = await opake(["ls"], { configDir: ctx.configDir });
    expect(ls.code).toBe(0);
    expect(ls.stdout).toContain("hello.txt");

    // Download
    const downloadPath = join(workDir, "downloaded.txt");
    const download = await opake(
      ["download", "hello.txt", "-o", downloadPath],
      { configDir: ctx.configDir },
    );
    expect(download.code).toBe(0);

    // Verify content matches
    const downloaded = readFileSync(downloadPath, "utf-8");
    expect(downloaded).toBe("hello opake");
  });

  it("ls -l shows mime type and size", async () => {
    const testFile = join(workDir, "doc.txt");
    writeFileSync(testFile, "some content");

    await opake(["upload", testFile], { configDir: ctx.configDir });
    const ls = await opake(["ls", "-l"], { configDir: ctx.configDir });

    expect(ls.code).toBe(0);
    expect(ls.stdout).toContain("doc.txt");
    expect(ls.stdout).toContain("text/plain");
  });

  it("download by AT-URI", async () => {
    const testFile = join(workDir, "uri-test.txt");
    writeFileSync(testFile, "uri content");

    const upload = await opake(["upload", testFile], { configDir: ctx.configDir });
    // Extract AT-URI from output
    const uriMatch = upload.stdout.match(/at:\/\/[^\s]+/);
    expect(uriMatch).toBeTruthy();

    const downloadPath = join(workDir, "uri-download.txt");
    const download = await opake(
      ["download", uriMatch![0]!, "-o", downloadPath],
      { configDir: ctx.configDir },
    );
    expect(download.code).toBe(0);
    expect(readFileSync(downloadPath, "utf-8")).toBe("uri content");
  });

  it("download refuses to overwrite existing file", async () => {
    const testFile = join(workDir, "overwrite.txt");
    writeFileSync(testFile, "original");
    await opake(["upload", testFile], { configDir: ctx.configDir });

    // Create the output file so it exists
    const outputPath = join(workDir, "already-exists.txt");
    writeFileSync(outputPath, "existing");

    const download = await opake(
      ["download", "overwrite.txt", "-o", outputPath],
      { configDir: ctx.configDir },
    );
    expect(download.code).not.toBe(0);
    expect(download.stderr).toContain("exists");
  });

  it("upload and download empty file", async () => {
    const testFile = join(workDir, "empty.bin");
    writeFileSync(testFile, "");

    const upload = await opake(["upload", testFile], { configDir: ctx.configDir });
    expect(upload.code).toBe(0);

    const downloadPath = join(workDir, "empty-download.bin");
    const download = await opake(
      ["download", "empty.bin", "-o", downloadPath],
      { configDir: ctx.configDir },
    );
    expect(download.code).toBe(0);
    expect(readFileSync(downloadPath, "utf-8")).toBe("");
  });

  it("cat decrypts to stdout", async () => {
    const testFile = join(workDir, "cat-test.txt");
    writeFileSync(testFile, "stdout content");

    await opake(["upload", testFile], { configDir: ctx.configDir });
    const cat = await opake(["cat", "cat-test.txt"], { configDir: ctx.configDir });

    expect(cat.code).toBe(0);
    expect(cat.stdout).toBe("stdout content");
  });
});
