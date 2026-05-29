import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";
import { findParentUri, ancestorsOf, findDocumentUriByRkey } from "../../src/lib/directoryTree";
import type { DirectoryEntry, DirectoryTreeSnapshot } from "../../src/lib/pdsTypes";

const DID = "did:plc:test";
const dir = (rkey: string) => `at://${DID}/app.opake.directory/${rkey}`;
const doc = (rkey: string) => `at://${DID}/app.opake.document/${rkey}`;

// Fixture shape mirrors the SDK's DirectoryInfo: entries are typed
// (`{uri, type}`), and every non-root directory carries a `parentUri`.
// Callers pass child rkey lists; the helper expands them into typed
// entries and fills `parentUri` automatically.
interface RawDir {
  readonly name: string;
  readonly children: ReadonlyArray<{ readonly rkey: string; readonly type: "directory" | "document" }>;
}

function makeSnapshot(rootRkey: string, dirs: Record<string, RawDir>): DirectoryTreeSnapshot {
  const directories: Record<
    string,
    { readonly name: string; readonly entries: readonly DirectoryEntry[]; readonly parentUri: string | null }
  > = {};

  const parentByRkey = new Map<string, string>();
  for (const [parentRkey, info] of Object.entries(dirs)) {
    for (const child of info.children) {
      if (child.type === "directory") parentByRkey.set(child.rkey, parentRkey);
    }
  }

  for (const [rkey, info] of Object.entries(dirs)) {
    const entries: readonly DirectoryEntry[] = info.children.map((c) => ({
      uri: c.type === "directory" ? dir(c.rkey) : doc(c.rkey),
      type: c.type,
    }));
    const parentRkey = parentByRkey.get(rkey);
    directories[dir(rkey)] = {
      name: info.name,
      entries,
      parentUri: parentRkey ? dir(parentRkey) : null,
    };
  }
  return { rootUri: dir(rootRkey), directories };
}

// ---------------------------------------------------------------------------
// findParentUri
// ---------------------------------------------------------------------------

describe("findParentUri", () => {
  const snapshot = makeSnapshot("self", {
    self: {
      name: "/",
      children: [
        { rkey: "photos", type: "directory" },
        { rkey: "readme", type: "document" },
      ],
    },
    photos: {
      name: "Photos",
      children: [
        { rkey: "pic1", type: "document" },
        { rkey: "pic2", type: "document" },
      ],
    },
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
    self: { name: "/", children: [{ rkey: "projects", type: "directory" }] },
    projects: { name: "Projects", children: [{ rkey: "opake", type: "directory" }] },
    opake: { name: "Opake", children: [{ rkey: "readme", type: "document" }] },
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
      self: { name: "/", children: [{ rkey: "a", type: "directory" }] },
      a: { name: "A", children: [{ rkey: "b", type: "directory" }] },
      b: { name: "B", children: [{ rkey: "c", type: "directory" }] },
      c: { name: "C", children: [{ rkey: "leaf", type: "document" }] },
    });

    const ancestors = ancestorsOf(deep, dir("c"));
    expect(ancestors).toHaveLength(2);
    expect(ancestors[0].name).toBe("A");
    expect(ancestors[1].name).toBe("B");
  });
});

// ---------------------------------------------------------------------------
// findDocumentUriByRkey
// ---------------------------------------------------------------------------

describe("findDocumentUriByRkey", () => {
  const snapshot = makeSnapshot("self", {
    self: {
      name: "/",
      children: [
        { rkey: "photos", type: "directory" },
        { rkey: "readme", type: "document" },
      ],
    },
    photos: { name: "Photos", children: [{ rkey: "vacation", type: "document" }] },
  });

  it("returns kind:found for a document at root", () => {
    expect(findDocumentUriByRkey(snapshot, "readme")).toEqual({
      kind: "found",
      uri: doc("readme"),
    });
  });

  it("returns kind:found for a document deeper in the tree", () => {
    expect(findDocumentUriByRkey(snapshot, "vacation")).toEqual({
      kind: "found",
      uri: doc("vacation"),
    });
  });

  it("returns kind:not-found for an unknown rkey", () => {
    expect(findDocumentUriByRkey(snapshot, "missing")).toEqual({ kind: "not-found" });
  });

  it("ignores directories — only documents resolve by rkey", () => {
    expect(findDocumentUriByRkey(snapshot, "photos")).toEqual({ kind: "not-found" });
  });

  describe("ambiguous matches", () => {
    // Distinct authors writing the same rkey under different DIDs is a
    // data-integrity anomaly (TID rkeys make collisions astronomical),
    // but if it ever happens the helper must surface it as a distinct
    // signal — not silently collapse to either branch.
    const collision = (): DirectoryTreeSnapshot => {
      const owner = "did:plc:owner";
      const alice = "did:plc:alice";
      const bob = "did:plc:bob";
      const ownerDir = `at://${owner}/app.opake.directory/root`;
      return {
        rootUri: ownerDir,
        directories: {
          [ownerDir]: {
            name: "/",
            entries: [
              { uri: `at://${alice}/app.opake.document/collide`, type: "document" },
              { uri: `at://${bob}/app.opake.document/collide`, type: "document" },
            ],
            parentUri: null,
          },
        },
      };
    };

    let warnSpy: ReturnType<typeof vi.spyOn>;

    beforeEach(() => {
      warnSpy = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    });

    afterEach(() => {
      warnSpy.mockRestore();
    });

    it("returns kind:ambiguous when multiple documents share an rkey", () => {
      const result = findDocumentUriByRkey(collision(), "collide");
      expect(result.kind).toBe("ambiguous");
      if (result.kind === "ambiguous") {
        expect(result.uris).toHaveLength(2);
      }
    });

    it("logs a warning so the anomaly is observable", () => {
      findDocumentUriByRkey(collision(), "collide");
      expect(warnSpy).toHaveBeenCalledTimes(1);
      const [message] = warnSpy.mock.calls[0] ?? [];
      expect(String(message)).toContain("findDocumentUriByRkey");
    });
  });
});
