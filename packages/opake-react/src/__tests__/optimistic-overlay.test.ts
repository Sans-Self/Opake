import { describe, expect, it, vi } from "vitest";
import type { DirectoryTreeSnapshot } from "@opake/sdk";
import { OptimisticOverlay, scopeKey } from "../optimistic-overlay";

const emptySnapshot: DirectoryTreeSnapshot = {
  rootUri: "at://did:plc:test/app.opake.directory/self",
  directories: {
    "at://did:plc:test/app.opake.directory/self": {
      name: "Cabinet",
      entries: [
        { uri: "at://did:plc:test/app.opake.document/a", type: "document" },
        { uri: "at://did:plc:test/app.opake.document/b", type: "document" },
      ],
      parentUri: null,
    },
  },
};

const ROOT_URI = "at://did:plc:test/app.opake.directory/self";
const DOC_A = "at://did:plc:test/app.opake.document/a";
const DOC_B = "at://did:plc:test/app.opake.document/b";

function removeDoc(uri: string) {
  return (snap: DirectoryTreeSnapshot): DirectoryTreeSnapshot => {
    const dir = snap.directories[ROOT_URI];
    if (!dir) return snap;
    return {
      ...snap,
      directories: {
        ...snap.directories,
        [ROOT_URI]: { ...dir, entries: dir.entries.filter((e) => e.uri !== uri) },
      },
    };
  };
}

describe("scopeKey", () => {
  it("maps null to 'cabinet'", () => {
    expect(scopeKey(null)).toBe("cabinet");
  });

  it("returns the keyring URI for a workspace", () => {
    expect(scopeKey("at://did:plc:test/app.opake.keyring/xyz")).toBe(
      "at://did:plc:test/app.opake.keyring/xyz",
    );
  });
});

describe("OptimisticOverlay", () => {
  it("returns the base snapshot when no patches are active", () => {
    const overlay = new OptimisticOverlay();
    expect(overlay.project("cabinet", emptySnapshot)).toBe(emptySnapshot);
  });

  it("applies a single patch", () => {
    const overlay = new OptimisticOverlay();
    overlay.apply("cabinet", removeDoc(DOC_A));

    const projected = overlay.project("cabinet", emptySnapshot);
    expect(projected.directories[ROOT_URI]?.entries.map((e) => e.uri)).toEqual([DOC_B]);
  });

  it("composes multiple patches in order", () => {
    const overlay = new OptimisticOverlay();
    overlay.apply("cabinet", removeDoc(DOC_A));
    overlay.apply("cabinet", removeDoc(DOC_B));

    const projected = overlay.project("cabinet", emptySnapshot);
    expect(projected.directories[ROOT_URI]?.entries).toEqual([]);
  });

  it("releases a patch and reverts to base", () => {
    const overlay = new OptimisticOverlay();
    const release = overlay.apply("cabinet", removeDoc(DOC_A));
    expect(overlay.patchCount("cabinet")).toBe(1);

    release();

    expect(overlay.patchCount("cabinet")).toBe(0);
    expect(overlay.project("cabinet", emptySnapshot)).toBe(emptySnapshot);
  });

  it("keeps remaining patches when one is released", () => {
    const overlay = new OptimisticOverlay();
    const releaseA = overlay.apply("cabinet", removeDoc(DOC_A));
    overlay.apply("cabinet", removeDoc(DOC_B));

    releaseA();

    expect(overlay.patchCount("cabinet")).toBe(1);
    expect(overlay.project("cabinet", emptySnapshot).directories[ROOT_URI]?.entries.map((e) => e.uri)).toEqual([DOC_A]);
  });

  it("release is idempotent", () => {
    const overlay = new OptimisticOverlay();
    const release = overlay.apply("cabinet", removeDoc(DOC_A));
    overlay.apply("cabinet", removeDoc(DOC_B));

    release();
    release(); // second release must not remove an unrelated patch

    expect(overlay.patchCount("cabinet")).toBe(1);
  });

  it("isolates scopes", () => {
    const overlay = new OptimisticOverlay();
    overlay.apply("cabinet", removeDoc(DOC_A));

    expect(overlay.patchCount("cabinet")).toBe(1);
    expect(overlay.patchCount("workspace:xyz")).toBe(0);
    expect(overlay.project("workspace:xyz", emptySnapshot)).toBe(emptySnapshot);
  });

  it("notifies subscribers on apply and release", () => {
    const overlay = new OptimisticOverlay();
    const callback = vi.fn();
    overlay.subscribe("cabinet", callback);

    const release = overlay.apply("cabinet", removeDoc(DOC_A));
    expect(callback).toHaveBeenCalledTimes(1);

    release();
    expect(callback).toHaveBeenCalledTimes(2);
  });

  it("only notifies subscribers of the matching scope", () => {
    const overlay = new OptimisticOverlay();
    const cabinetCb = vi.fn();
    const workspaceCb = vi.fn();
    overlay.subscribe("cabinet", cabinetCb);
    overlay.subscribe("workspace:xyz", workspaceCb);

    overlay.apply("cabinet", removeDoc(DOC_A));

    expect(cabinetCb).toHaveBeenCalledTimes(1);
    expect(workspaceCb).not.toHaveBeenCalled();
  });

  it("stops notifying after unsubscribe", () => {
    const overlay = new OptimisticOverlay();
    const callback = vi.fn();
    const unsub = overlay.subscribe("cabinet", callback);

    overlay.apply("cabinet", removeDoc(DOC_A));
    expect(callback).toHaveBeenCalledTimes(1);

    unsub();

    overlay.apply("cabinet", removeDoc(DOC_B));
    expect(callback).toHaveBeenCalledTimes(1);
  });
});
