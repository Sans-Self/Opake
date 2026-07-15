import { describe, it, expect } from "vitest";
import { snapshotToFileItems } from "../../src/lib/fileContext";
import { isActionable, isEditable, isPreviewable, type FileItem } from "../../src/components/cabinet/types";
import type { DirectoryTreeSnapshot } from "../../src/lib/pdsTypes";
import type { DocumentMetadata } from "@opake/sdk";

// The optimistic overlay projects provisional entries carrying an explicit
// `pending` marker. These tests pin that the marker flows first-class through
// snapshotToFileItems onto FileItem, and that a pending entry is non-actionable
// — never a mutation target — regardless of its kind or decrypt state.
// spec:indexer-consistency § Client projections contain only indexer-confirmed state

const DID = "did:plc:test";
const rootDir = `at://${DID}/at.opake.directory/root`;
const realDocUri = `at://${DID}/at.opake.document/real`;
// Placeholder URIs the overlay mints; provisionality is carried by `pending`,
// not by these shapes — the test never inspects the URI to decide pending.
const pendingUploadUri = "pending:upload:notes.txt:123";
const pendingDirUri = "pending-dir:Photos:123";

const snapshot: DirectoryTreeSnapshot = {
  rootUri: rootDir,
  directories: {
    [rootDir]: {
      name: "root",
      parentUri: null,
      entries: [
        { uri: realDocUri, type: "document" },
        { uri: pendingUploadUri, type: "document", pending: true },
        { uri: pendingDirUri, type: "directory", pending: true },
      ],
    },
    [pendingDirUri]: { name: "Photos", entries: [], parentUri: rootDir },
  },
};

const metadata: Record<string, DocumentMetadata> = {
  [realDocUri]: {
    name: "real.txt",
    mimeType: "text/plain",
    size: 12,
    tags: [],
    description: null,
    createdAt: "2026-07-12T00:00:00Z",
    modifiedAt: "2026-07-12T00:00:00Z",
  },
};

function itemByUri(items: readonly FileItem[], uri: string): FileItem {
  const found = items.find((i) => i.uri === uri);
  if (!found) throw new Error(`no item for ${uri}`);
  return found;
}

describe("snapshotToFileItems pending marker", () => {
  const items = snapshotToFileItems(rootDir, snapshot, metadata);

  it("carries the overlay's pending marker onto both file and folder entries", () => {
    expect(itemByUri(items, pendingUploadUri).pending).toBe(true);
    expect(itemByUri(items, pendingDirUri).pending).toBe(true);
  });

  it("leaves indexer-confirmed entries unmarked", () => {
    expect(itemByUri(items, realDocUri).pending).toBeFalsy();
  });

  it("makes a provisional directory non-actionable despite rendering as a normal folder", () => {
    const pendingFolder = itemByUri(items, pendingDirUri);
    expect(pendingFolder.kind).toBe("folder");
    expect(pendingFolder.decrypted).toBe(true); // renders like a real folder…
    expect(isActionable(pendingFolder)).toBe(false); // …but cannot be operated on
  });

  it("makes a provisional upload non-actionable", () => {
    expect(isActionable(itemByUri(items, pendingUploadUri))).toBe(false);
  });

  it("keeps a real, decrypted document actionable", () => {
    expect(isActionable(itemByUri(items, realDocUri))).toBe(true);
  });
});

describe("provisional entries fail every operation gate", () => {
  const noteItem: FileItem = {
    id: "x",
    uri: pendingUploadUri,
    name: "note.md",
    kind: "file",
    fileType: "note",
    encrypted: true,
    status: "private",
    modified: "",
    decrypted: true,
    tags: [],
    pending: true,
  };

  it("is neither editable nor previewable while pending", () => {
    expect(isEditable(noteItem)).toBe(false);
    expect(isPreviewable(noteItem)).toBe(false);
    // Same item, not pending → both gates open, proving `pending` is the cause.
    const settled = { ...noteItem, pending: false };
    expect(isEditable(settled)).toBe(true);
    expect(isPreviewable(settled)).toBe(true);
  });
});
