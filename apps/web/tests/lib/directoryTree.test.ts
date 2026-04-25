import { describe, it, expect } from "vitest";
import { findParentUri, ancestorsOf } from "../../src/lib/directoryTree";
import type { DirectoryTreeSnapshot } from "../../src/lib/pdsTypes";

const DID = "did:plc:test";
const dir = (rkey: string) => `at://${DID}/app.opake.directory/${rkey}`;
const doc = (rkey: string) => `at://${DID}/app.opake.document/${rkey}`;

function makeSnapshot(
  rootRkey: string,
  dirs: Record<string, { name: string; entries: string[] }>,
): DirectoryTreeSnapshot {
  const directories: Record<string, { name: string; entries: string[] }> = {};
  for (const [rkey, entry] of Object.entries(dirs)) {
    directories[dir(rkey)] = entry;
  }
  return { rootUri: dir(rootRkey), directories };
}

// ---------------------------------------------------------------------------
// findParentUri
// ---------------------------------------------------------------------------

describe("findParentUri", () => {
  const snapshot = makeSnapshot("self", {
    self: { name: "/", entries: [dir("photos"), doc("readme")] },
    photos: { name: "Photos", entries: [doc("pic1"), doc("pic2")] },
  });

  it("finds parent of a document in root", () => {
    expect(findParentUri(snapshot, doc("readme"))).toBe(dir("self"));
  });

  it("finds parent of a document in subdirectory", () => {
    expect(findParentUri(snapshot, doc("pic1"))).toBe(dir("photos"));
  });

  it("finds parent of a subdirectory", () => {
    expect(findParentUri(snapshot, dir("photos"))).toBe(dir("self"));
  });

  it("returns null for root directory", () => {
    expect(findParentUri(snapshot, dir("self"))).toBeNull();
  });

  it("returns null for unknown URI", () => {
    expect(findParentUri(snapshot, doc("nonexistent"))).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// ancestorsOf
// ---------------------------------------------------------------------------

describe("ancestorsOf", () => {
  const snapshot = makeSnapshot("self", {
    self: { name: "/", entries: [dir("projects")] },
    projects: { name: "Projects", entries: [dir("opake")] },
    opake: { name: "Opake", entries: [doc("readme")] },
  });

  it("returns empty for null input", () => {
    expect(ancestorsOf(snapshot, null)).toEqual([]);
  });

  it("returns empty for root's direct child", () => {
    // projects is a direct child of root — no ancestors between it and root
    expect(ancestorsOf(snapshot, dir("projects"))).toEqual([]);
  });

  it("returns parent chain excluding root", () => {
    const ancestors = ancestorsOf(snapshot, dir("opake"));
    expect(ancestors).toEqual([
      {
        uri: dir("projects"),
        name: "Projects",
        rkey: "projects",
      },
    ]);
  });

  it("returns empty for unknown directory", () => {
    expect(ancestorsOf(snapshot, dir("unknown"))).toEqual([]);
  });

  it("builds multi-level ancestor chain", () => {
    const deep = makeSnapshot("self", {
      self: { name: "/", entries: [dir("a")] },
      a: { name: "A", entries: [dir("b")] },
      b: { name: "B", entries: [dir("c")] },
      c: { name: "C", entries: [doc("leaf")] },
    });

    const ancestors = ancestorsOf(deep, dir("c"));
    expect(ancestors).toHaveLength(2);
    expect(ancestors[0].name).toBe("A");
    expect(ancestors[1].name).toBe("B");
  });
});
