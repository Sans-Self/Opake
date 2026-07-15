import { describe, it, expect } from "vitest";
import {
  MAX_NAME_BYTES,
  NameAlreadyExistsError,
  buildSplatPath,
  checkNameAvailability,
  describeValidationReason,
  directoryNamePathSegments,
  findNameConflict,
  normalizeName,
  parseSplatPath,
  partialResolveNamePath,
  resolveDirectoryFromNamePath,
  validateName,
  type DocumentNameLookup,
} from "../../src/lib/namePath";
import type {
  DirectoryEntry,
  DirectoryInfo,
  DirectoryTreeSnapshot,
} from "../../src/lib/pdsTypes";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const DID = "did:plc:test";
const dirUri = (rkey: string) => `at://${DID}/at.opake.directory/${rkey}`;
const docUri = (rkey: string) => `at://${DID}/at.opake.document/${rkey}`;

type EntrySeed = { readonly rkey: string; readonly type: "directory" | "document" };

function makeEntries(seeds: readonly EntrySeed[]): readonly DirectoryEntry[] {
  return seeds.map((s) =>
    s.type === "directory"
      ? { uri: dirUri(s.rkey), type: "directory" }
      : { uri: docUri(s.rkey), type: "document" },
  );
}

interface DirSeed {
  readonly rkey: string;
  readonly name: string;
  readonly parent: string | null;
  readonly entries?: readonly EntrySeed[];
}

function makeSnapshot(rootRkey: string, dirs: readonly DirSeed[]): DirectoryTreeSnapshot {
  const directories: Record<string, DirectoryInfo> = {};
  for (const seed of dirs) {
    directories[dirUri(seed.rkey)] = {
      name: seed.name,
      entries: makeEntries(seed.entries ?? []),
      parentUri: seed.parent === null ? null : dirUri(seed.parent),
    };
  }
  return { rootUri: dirUri(rootRkey), directories };
}

// ---------------------------------------------------------------------------
// normalizeName
// ---------------------------------------------------------------------------

describe("normalizeName", () => {
  it("trims surrounding whitespace", () => {
    expect(normalizeName("  hello  ")).toBe("hello");
  });

  it("preserves inner whitespace", () => {
    expect(normalizeName("hello world")).toBe("hello world");
  });

  it("NFC-normalizes composed vs decomposed forms to equal output", () => {
    // U+00E9 (composed é) vs U+0065 + U+0301 (decomposed e + combining acute)
    const composed = "caf\u00E9";
    const decomposed = "cafe\u0301";
    expect(normalizeName(composed)).toBe(normalizeName(decomposed));
  });

  it("returns empty string for whitespace-only input", () => {
    expect(normalizeName("   ")).toBe("");
  });
});

// ---------------------------------------------------------------------------
// validateName
// ---------------------------------------------------------------------------

describe("validateName", () => {
  it("accepts a simple ASCII name", () => {
    const result = validateName("projects");
    expect(result).toEqual({ ok: true, normalized: "projects" });
  });

  it("accepts a name with spaces and punctuation", () => {
    const result = validateName("My Project (v2).draft");
    expect(result).toEqual({ ok: true, normalized: "My Project (v2).draft" });
  });

  it("accepts non-Latin scripts", () => {
    expect(validateName("文書")).toEqual({ ok: true, normalized: "文書" });
    expect(validateName("café")).toEqual({ ok: true, normalized: "café" });
    expect(validateName("Москва")).toEqual({ ok: true, normalized: "Москва" });
  });

  it("trims surrounding whitespace before validating", () => {
    const result = validateName("  projects  ");
    expect(result).toEqual({ ok: true, normalized: "projects" });
  });

  it("rejects empty input", () => {
    expect(validateName("")).toEqual({ ok: false, reason: "empty" });
  });

  it("rejects whitespace-only input", () => {
    expect(validateName("   ")).toEqual({ ok: false, reason: "empty" });
  });

  it("rejects reserved names `.` and `..`", () => {
    expect(validateName(".")).toEqual({ ok: false, reason: "reserved" });
    expect(validateName("..")).toEqual({ ok: false, reason: "reserved" });
  });

  it("rejects the URL file marker as a name", () => {
    // FILE_MARKER doubles as a reserved name so the parser's
    // structural separator is guaranteed not to collide with any
    // user-provided name. Don't drop this — it's load-bearing for
    // the parseSplatPath roundtrip.
    expect(validateName("__file__")).toEqual({ ok: false, reason: "reserved" });
  });

  it("rejects names containing `/`", () => {
    expect(validateName("foo/bar")).toEqual({ ok: false, reason: "forbidden-char" });
  });

  it("rejects names containing control characters", () => {
    expect(validateName("foo\nbar")).toEqual({ ok: false, reason: "forbidden-char" });
    expect(validateName("foo\rbar")).toEqual({ ok: false, reason: "forbidden-char" });
    expect(validateName("foo\tbar")).toEqual({ ok: false, reason: "forbidden-char" });
    expect(validateName("foo\0bar")).toEqual({ ok: false, reason: "forbidden-char" });
  });

  it("accepts names with non-forbidden punctuation that Windows would reject", () => {
    // < > : " | ? * are fine in our resolver — we control routing.
    expect(validateName("question?")).toEqual({ ok: true, normalized: "question?" });
    expect(validateName("ratio 1:2")).toEqual({ ok: true, normalized: "ratio 1:2" });
  });

  it("rejects names exceeding the byte limit", () => {
    const longName = "a".repeat(MAX_NAME_BYTES + 1);
    expect(validateName(longName)).toEqual({ ok: false, reason: "too-long" });
  });

  it("counts bytes not code points for the length limit", () => {
    // Each emoji is 4 UTF-8 bytes. 64 emoji = 256 bytes.
    const emojiName = "🌸".repeat(64);
    expect(validateName(emojiName)).toEqual({ ok: false, reason: "too-long" });
  });

  it("accepts a name at exactly the byte limit", () => {
    const limitName = "a".repeat(MAX_NAME_BYTES);
    expect(validateName(limitName)).toEqual({ ok: true, normalized: limitName });
  });
});

// ---------------------------------------------------------------------------
// describeValidationReason
// ---------------------------------------------------------------------------

describe("describeValidationReason", () => {
  it("produces a non-empty message for every reason", () => {
    const reasons = ["empty", "reserved", "forbidden-char", "too-long"] as const;
    for (const r of reasons) {
      expect(describeValidationReason(r).length).toBeGreaterThan(0);
    }
  });
});

// ---------------------------------------------------------------------------
// resolveDirectoryFromNamePath
// ---------------------------------------------------------------------------

describe("resolveDirectoryFromNamePath", () => {
  const snapshot = makeSnapshot("self", [
    {
      rkey: "self",
      name: "/",
      parent: null,
      entries: [
        { rkey: "projects", type: "directory" },
        { rkey: "photos", type: "directory" },
        { rkey: "readme", type: "document" },
      ],
    },
    {
      rkey: "projects",
      name: "Projects",
      parent: "self",
      entries: [{ rkey: "q3", type: "directory" }],
    },
    {
      rkey: "photos",
      name: "Photos",
      parent: "self",
      entries: [],
    },
    {
      rkey: "q3",
      name: "Q3 Report",
      parent: "projects",
      entries: [{ rkey: "notes", type: "directory" }],
    },
    {
      rkey: "notes",
      name: "notes",
      parent: "q3",
      entries: [],
    },
  ]);

  it("returns null for an empty name path", () => {
    expect(resolveDirectoryFromNamePath(snapshot, [])).toBeNull();
  });

  it("resolves a single segment", () => {
    expect(resolveDirectoryFromNamePath(snapshot, ["Projects"])).toBe(dirUri("projects"));
  });

  it("resolves a deep path", () => {
    expect(resolveDirectoryFromNamePath(snapshot, ["Projects", "Q3 Report", "notes"])).toBe(
      dirUri("notes"),
    );
  });

  it("returns null when a segment doesn't match", () => {
    expect(resolveDirectoryFromNamePath(snapshot, ["Projects", "missing"])).toBeNull();
  });

  it("is case-sensitive", () => {
    expect(resolveDirectoryFromNamePath(snapshot, ["projects"])).toBeNull();
  });

  it("normalizes NFC before comparing", () => {
    const decomposedSnap = makeSnapshot("self", [
      {
        rkey: "self",
        name: "/",
        parent: null,
        entries: [{ rkey: "cafe", type: "directory" }],
      },
      // Stored as decomposed form
      { rkey: "cafe", name: "cafe\u0301", parent: "self" },
    ]);
    expect(resolveDirectoryFromNamePath(decomposedSnap, ["caf\u00E9"])).toBe(dirUri("cafe"));
  });

  it("treats whitespace-padded segments equivalently after normalize", () => {
    expect(resolveDirectoryFromNamePath(snapshot, ["  Projects  "])).toBe(dirUri("projects"));
  });

  it("ignores document entries during the walk", () => {
    // `readme` is a document; should not match as a path segment
    expect(resolveDirectoryFromNamePath(snapshot, ["readme"])).toBeNull();
  });

  it("returns null when root is missing", () => {
    const noRoot: DirectoryTreeSnapshot = { rootUri: null, directories: {} };
    expect(resolveDirectoryFromNamePath(noRoot, ["anything"])).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// directoryNamePathSegments
// ---------------------------------------------------------------------------

describe("directoryNamePathSegments", () => {
  const snapshot = makeSnapshot("self", [
    {
      rkey: "self",
      name: "/",
      parent: null,
      entries: [{ rkey: "projects", type: "directory" }],
    },
    {
      rkey: "projects",
      name: "Projects",
      parent: "self",
      entries: [{ rkey: "q3", type: "directory" }],
    },
    { rkey: "q3", name: "Q3 Report", parent: "projects" },
  ]);

  it("returns null for the root", () => {
    expect(directoryNamePathSegments(snapshot, dirUri("self"))).toBeNull();
  });

  it("returns null for null input", () => {
    expect(directoryNamePathSegments(snapshot, null)).toBeNull();
  });

  it("returns null for a missing URI", () => {
    expect(directoryNamePathSegments(snapshot, dirUri("nonexistent"))).toBeNull();
  });

  it("returns a single segment for root's direct child", () => {
    expect(directoryNamePathSegments(snapshot, dirUri("projects"))).toEqual(["Projects"]);
  });

  it("returns a deep path in root → leaf order", () => {
    expect(directoryNamePathSegments(snapshot, dirUri("q3"))).toEqual(["Projects", "Q3 Report"]);
  });

  it("NFC-normalizes stored decomposed names", () => {
    const decomposed = makeSnapshot("self", [
      {
        rkey: "self",
        name: "/",
        parent: null,
        entries: [{ rkey: "cafe", type: "directory" }],
      },
      { rkey: "cafe", name: "cafe\u0301", parent: "self" },
    ]);
    expect(directoryNamePathSegments(decomposed, dirUri("cafe"))).toEqual(["caf\u00E9"]);
  });

  it("roundtrips with resolveDirectoryFromNamePath", () => {
    const segments = directoryNamePathSegments(snapshot, dirUri("q3"));
    expect(segments).not.toBeNull();
    expect(resolveDirectoryFromNamePath(snapshot, segments!)).toBe(dirUri("q3"));
  });
});

// ---------------------------------------------------------------------------
// parseSplatPath / buildSplatPath
// ---------------------------------------------------------------------------

describe("parseSplatPath", () => {
  it("parses an empty splat as a root directory view", () => {
    expect(parseSplatPath("")).toEqual({ dirSegments: [], fileSegment: null });
    expect(parseSplatPath(undefined)).toEqual({ dirSegments: [], fileSegment: null });
  });

  it("parses a single directory segment", () => {
    expect(parseSplatPath("projects")).toEqual({
      dirSegments: ["projects"],
      fileSegment: null,
    });
  });

  it("parses a deep directory path", () => {
    expect(parseSplatPath("projects/q3/notes")).toEqual({
      dirSegments: ["projects", "q3", "notes"],
      fileSegment: null,
    });
  });

  it("parses a file leaf with no directory path", () => {
    expect(parseSplatPath("__file__/report.pdf")).toEqual({
      dirSegments: [],
      fileSegment: "report.pdf",
    });
  });

  it("parses a file leaf nested under directories", () => {
    expect(parseSplatPath("projects/q3/__file__/report.pdf")).toEqual({
      dirSegments: ["projects", "q3"],
      fileSegment: "report.pdf",
    });
  });

  it("treats a directory literally named `f` as a regular segment", () => {
    // `f` is no longer the marker — it's just a directory name. The
    // structural marker is `__file__` (reserved by the validator).
    expect(parseSplatPath("f/projects")).toEqual({
      dirSegments: ["f", "projects"],
      fileSegment: null,
    });
    expect(parseSplatPath("projects/f")).toEqual({
      dirSegments: ["projects", "f"],
      fileSegment: null,
    });
  });

  it("resolves files literally named `f` cleanly via the marker", () => {
    // The bug the marker change fixes: a file actually named `f`
    // can be addressed via `__file__/f` and roundtrips.
    expect(parseSplatPath("__file__/f")).toEqual({
      dirSegments: [],
      fileSegment: "f",
    });
    expect(parseSplatPath("projects/__file__/f")).toEqual({
      dirSegments: ["projects"],
      fileSegment: "f",
    });
  });

  it("ignores extra segments after the file leaf", () => {
    expect(parseSplatPath("projects/__file__/report.pdf/extra/junk")).toEqual({
      dirSegments: ["projects"],
      fileSegment: "report.pdf",
    });
  });

  it("drops empty segments from double slashes and trailing slashes", () => {
    expect(parseSplatPath("//projects///q3//")).toEqual({
      dirSegments: ["projects", "q3"],
      fileSegment: null,
    });
  });

  it("NFC-normalizes segments", () => {
    // decomposed `é` (e + combining acute) → composed
    expect(parseSplatPath("cafe\u0301/__file__/menu")).toEqual({
      dirSegments: ["caf\u00E9"],
      fileSegment: "menu",
    });
  });

  it("treats trailing marker with no leaf as a directory walk (dropping marker)", () => {
    // The marker can't legitimately appear as a name (reserved), but
    // a malformed URL ending in the marker shouldn't crash — fall back
    // to interpreting it as a (broken) directory walk so the
    // not-found UX renders cleanly.
    expect(parseSplatPath("projects/__file__")).toEqual({
      dirSegments: ["projects", "__file__"],
      fileSegment: null,
    });
  });
});

describe("buildSplatPath", () => {
  it("builds a directory-only path", () => {
    expect(buildSplatPath(["projects", "q3"])).toBe("projects/q3");
  });

  it("builds an empty path for empty input", () => {
    expect(buildSplatPath([])).toBe("");
  });

  it("appends the file marker and leaf for file URLs", () => {
    expect(buildSplatPath(["projects", "q3"], "report.pdf")).toBe(
      "projects/q3/__file__/report.pdf",
    );
  });

  it("handles a file leaf with no directories", () => {
    expect(buildSplatPath([], "report.pdf")).toBe("__file__/report.pdf");
  });

  it("treats null/empty/undefined file leaf the same as none", () => {
    expect(buildSplatPath(["projects"], null)).toBe("projects");
    expect(buildSplatPath(["projects"], undefined)).toBe("projects");
    expect(buildSplatPath(["projects"], "")).toBe("projects");
  });

  it("roundtrips through parseSplatPath", () => {
    const cases = [
      { dirs: [], file: null },
      { dirs: ["projects"], file: null },
      { dirs: ["projects", "q3"], file: null },
      { dirs: [], file: "report.pdf" },
      { dirs: ["projects", "q3"], file: "report.pdf" },
      { dirs: ["f", "projects"], file: "report.pdf" },
      // Files literally named `f` are the bug the marker change fixed.
      { dirs: [], file: "f" },
      { dirs: ["f"], file: "f" },
      { dirs: ["f", "f", "f"], file: "f" },
      { dirs: ["projects", "f"], file: "report.pdf" },
    ];
    for (const { dirs, file } of cases) {
      const built = buildSplatPath(dirs, file);
      const parsed = parseSplatPath(built);
      expect(parsed.dirSegments).toEqual(dirs);
      expect(parsed.fileSegment).toBe(file);
    }
  });
});

// ---------------------------------------------------------------------------
// partialResolveNamePath
// ---------------------------------------------------------------------------

describe("partialResolveNamePath", () => {
  const snapshot = makeSnapshot("self", [
    {
      rkey: "self",
      name: "/",
      parent: null,
      entries: [{ rkey: "projects", type: "directory" }],
    },
    {
      rkey: "projects",
      name: "Projects",
      parent: "self",
      entries: [{ rkey: "q3", type: "directory" }],
    },
    { rkey: "q3", name: "Q3 Report", parent: "projects" },
  ]);

  it("reports zero depth and root URI for an unresolvable first segment", () => {
    expect(partialResolveNamePath(snapshot, ["missing"])).toEqual({
      resolvedDepth: 0,
      resolvedUri: dirUri("self"),
    });
  });

  it("reports full depth when the entire path resolves", () => {
    expect(partialResolveNamePath(snapshot, ["Projects", "Q3 Report"])).toEqual({
      resolvedDepth: 2,
      resolvedUri: dirUri("q3"),
    });
  });

  it("reports partial depth and the deepest reached URI when a mid segment fails", () => {
    expect(partialResolveNamePath(snapshot, ["Projects", "missing", "deeper"])).toEqual({
      resolvedDepth: 1,
      resolvedUri: dirUri("projects"),
    });
  });

  it("reports zero depth and null when the snapshot has no root", () => {
    const noRoot: DirectoryTreeSnapshot = { rootUri: null, directories: {} };
    expect(partialResolveNamePath(noRoot, ["whatever"])).toEqual({
      resolvedDepth: 0,
      resolvedUri: null,
    });
  });

  it("returns root for an empty name path", () => {
    expect(partialResolveNamePath(snapshot, [])).toEqual({
      resolvedDepth: 0,
      resolvedUri: dirUri("self"),
    });
  });
});

// ---------------------------------------------------------------------------
// findNameConflict
// ---------------------------------------------------------------------------

describe("findNameConflict", () => {
  const parentRkey = "parent";
  const subdirRkey = "subdir";
  const docRkey = "doc";

  const snapshot = makeSnapshot("root", [
    {
      rkey: "root",
      name: "/",
      parent: null,
      entries: [{ rkey: parentRkey, type: "directory" }],
    },
    {
      rkey: parentRkey,
      name: "Parent",
      parent: "root",
      entries: [
        { rkey: subdirRkey, type: "directory" },
        { rkey: docRkey, type: "document" },
      ],
    },
    { rkey: subdirRkey, name: "Existing Folder", parent: parentRkey },
  ]);

  const docMetadata: DocumentNameLookup = {
    [docUri(docRkey)]: { name: "report.pdf" },
  };

  it("returns null when no entry has the candidate name", () => {
    expect(findNameConflict(snapshot, dirUri(parentRkey), "fresh", docMetadata)).toBeNull();
  });

  it("finds a directory conflict", () => {
    expect(findNameConflict(snapshot, dirUri(parentRkey), "Existing Folder", docMetadata)).toEqual({
      uri: dirUri(subdirRkey),
      type: "directory",
    });
  });

  it("finds a document conflict", () => {
    expect(findNameConflict(snapshot, dirUri(parentRkey), "report.pdf", docMetadata)).toEqual({
      uri: docUri(docRkey),
      type: "document",
    });
  });

  it("is NFC-aware", () => {
    const decomposedSnap = makeSnapshot("root", [
      {
        rkey: "root",
        name: "/",
        parent: null,
        entries: [{ rkey: "cafe", type: "directory" }],
      },
      // stored decomposed
      { rkey: "cafe", name: "cafe\u0301", parent: "root" },
    ]);
    // candidate as composed
    expect(findNameConflict(decomposedSnap, dirUri("root"), "caf\u00E9", {})).toEqual({
      uri: dirUri("cafe"),
      type: "directory",
    });
  });

  it("excludes the renamed entry from conflict consideration", () => {
    expect(
      findNameConflict(
        snapshot,
        dirUri(parentRkey),
        "Existing Folder",
        docMetadata,
        dirUri(subdirRkey),
      ),
    ).toBeNull();
  });

  it("returns null for an unknown parent", () => {
    expect(findNameConflict(snapshot, dirUri("missing"), "anything", docMetadata)).toBeNull();
  });

  it("skips documents whose metadata is missing from the lookup", () => {
    // No metadata entry for the document → can't compare → skipped silently
    expect(findNameConflict(snapshot, dirUri(parentRkey), "report.pdf", {})).toBeNull();
  });

  it("is case-sensitive", () => {
    expect(findNameConflict(snapshot, dirUri(parentRkey), "existing folder", docMetadata)).toBeNull();
  });

  it("trims candidate whitespace before comparing", () => {
    expect(findNameConflict(snapshot, dirUri(parentRkey), "  Existing Folder  ", docMetadata)).toEqual(
      {
        uri: dirUri(subdirRkey),
        type: "directory",
      },
    );
  });

  it("resolves in-flight optimistic upload names via the pendingNameResolver", () => {
    // Snapshot patched with a placeholder upload entry, no metadata yet.
    // Without a resolver, the conflict slips through (the historical
    // bug the reviewer flagged). With the resolver, the in-flight name
    // is honored so a rapid double-click is caught client-side.
    const placeholderUri = "pending:upload:report.pdf:1234567890";
    const patched = makeSnapshot("root", [
      {
        rkey: "root",
        name: "/",
        parent: null,
        entries: [{ rkey: parentRkey, type: "directory" }],
      },
      {
        rkey: parentRkey,
        name: "Parent",
        parent: "root",
        // Inject a synthetic placeholder document directly into entries.
        // Bypasses makeEntries since the placeholder URI doesn't follow
        // the at:// docUri scheme — it's an optimistic-overlay artifact.
        entries: [{ rkey: subdirRkey, type: "directory" }],
      },
      { rkey: subdirRkey, name: "Existing Folder", parent: parentRkey },
    ]);
    // Splice the placeholder in manually.
    const withPlaceholder = {
      ...patched,
      directories: {
        ...patched.directories,
        [dirUri(parentRkey)]: {
          ...patched.directories[dirUri(parentRkey)],
          entries: [
            ...patched.directories[dirUri(parentRkey)].entries,
            { uri: placeholderUri, type: "document" as const },
          ],
        },
      },
    };

    const resolver = (uri: string) =>
      uri.startsWith("pending:upload:") ? "report.pdf" : null;

    // Without resolver: not detected.
    expect(findNameConflict(withPlaceholder, dirUri(parentRkey), "report.pdf", {})).toBeNull();

    // With resolver: detected.
    expect(
      findNameConflict(withPlaceholder, dirUri(parentRkey), "report.pdf", {}, undefined, resolver),
    ).toEqual({
      uri: placeholderUri,
      type: "document",
    });
  });

  it("prefers decrypted metadata over the pendingNameResolver", () => {
    // If both sources have a name for an entry, the metadata wins —
    // decrypted truth beats optimistic-overlay heuristic.
    const resolver = (_uri: string) => "overlay-name";
    expect(findNameConflict(snapshot, dirUri(parentRkey), "overlay-name", docMetadata, undefined, resolver)).toBeNull();
    expect(findNameConflict(snapshot, dirUri(parentRkey), "report.pdf", docMetadata, undefined, resolver)).toEqual({
      uri: docUri(docRkey),
      type: "document",
    });
  });
});

// ---------------------------------------------------------------------------
// checkNameAvailability (composed validate + conflict)
// ---------------------------------------------------------------------------

describe("checkNameAvailability", () => {
  const snapshot = makeSnapshot("root", [
    {
      rkey: "root",
      name: "/",
      parent: null,
      entries: [
        { rkey: "folder", type: "directory" },
        { rkey: "doc", type: "document" },
      ],
    },
    { rkey: "folder", name: "Existing", parent: "root" },
  ]);
  const docMetadata: DocumentNameLookup = {
    [docUri("doc")]: { name: "report.pdf" },
  };

  it("returns the normalized name when valid and no conflict", () => {
    const result = checkNameAvailability({
      snapshot,
      parentUri: dirUri("root"),
      rawName: "  fresh  ",
      documentMetadata: docMetadata,
    });
    expect(result).toEqual({ ok: true, normalized: "fresh" });
  });

  it("rejects invalid names with a UI-ready message", () => {
    const result = checkNameAvailability({
      snapshot,
      parentUri: dirUri("root"),
      rawName: "has/slash",
      documentMetadata: docMetadata,
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.message).toMatch(/`\/`/);
  });

  it("rejects when the snapshot is null with a helpful message", () => {
    const result = checkNameAvailability({
      snapshot: null,
      parentUri: dirUri("root"),
      rawName: "anything",
      documentMetadata: {},
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.message).toMatch(/not loaded/);
  });

  it("rejects directory-name conflicts", () => {
    const result = checkNameAvailability({
      snapshot,
      parentUri: dirUri("root"),
      rawName: "Existing",
      documentMetadata: docMetadata,
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.message).toMatch(/folder named "Existing"/);
  });

  it("rejects document-name conflicts", () => {
    const result = checkNameAvailability({
      snapshot,
      parentUri: dirUri("root"),
      rawName: "report.pdf",
      documentMetadata: docMetadata,
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.message).toMatch(/file named "report.pdf"/);
  });

  it("excludes the rename target from conflict consideration", () => {
    // Renaming "folder" to "Existing" — its own name — must pass.
    const result = checkNameAvailability({
      snapshot,
      parentUri: dirUri("root"),
      rawName: "Existing",
      documentMetadata: docMetadata,
      excludeUri: dirUri("folder"),
    });
    expect(result).toEqual({ ok: true, normalized: "Existing" });
  });
});

// ---------------------------------------------------------------------------
// NameAlreadyExistsError
// ---------------------------------------------------------------------------

describe("NameAlreadyExistsError", () => {
  it("renders a folder-specific message for directory conflicts", () => {
    const err = new NameAlreadyExistsError({
      attemptedName: "Projects",
      parentUri: dirUri("root"),
      conflict: { uri: dirUri("existing"), type: "directory" },
    });
    expect(err.message).toContain("folder");
    expect(err.message).toContain("Projects");
    expect(err.existingType).toBe("directory");
  });

  it("renders a file-specific message for document conflicts", () => {
    const err = new NameAlreadyExistsError({
      attemptedName: "report.pdf",
      parentUri: dirUri("root"),
      conflict: { uri: docUri("existing"), type: "document" },
    });
    expect(err.message).toContain("file");
    expect(err.message).toContain("report.pdf");
    expect(err.existingType).toBe("document");
  });
});
