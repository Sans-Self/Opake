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
const DOC_C = "at://did:plc:test/app.opake.document/c";

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

function addDoc(uri: string) {
  return (snap: DirectoryTreeSnapshot): DirectoryTreeSnapshot => {
    const dir = snap.directories[ROOT_URI];
    if (!dir) return snap;
    return {
      ...snap,
      directories: {
        ...snap.directories,
        [ROOT_URI]: {
          ...dir,
          entries: [...dir.entries, { uri, type: "document" as const }],
        },
      },
    };
  };
}

/** Base snapshot variant: same as empty but with `uri` also present. */
function snapshotWith(uri: string): DirectoryTreeSnapshot {
  return addDoc(uri)(emptySnapshot);
}

/** Base snapshot variant: empty minus `uri`. */
function snapshotWithout(uri: string): DirectoryTreeSnapshot {
  return removeDoc(uri)(emptySnapshot);
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
    const { release } = overlay.apply("cabinet", removeDoc(DOC_A));
    expect(overlay.patchCount("cabinet")).toBe(1);

    release();

    expect(overlay.patchCount("cabinet")).toBe(0);
    expect(overlay.project("cabinet", emptySnapshot)).toBe(emptySnapshot);
  });

  it("keeps remaining patches when one is released", () => {
    const overlay = new OptimisticOverlay();
    const { release: releaseA } = overlay.apply("cabinet", removeDoc(DOC_A));
    overlay.apply("cabinet", removeDoc(DOC_B));

    releaseA();

    expect(overlay.patchCount("cabinet")).toBe(1);
    expect(overlay.project("cabinet", emptySnapshot).directories[ROOT_URI]?.entries.map((e) => e.uri)).toEqual([DOC_A]);
  });

  it("release is idempotent and reports whether it removed a patch", () => {
    const overlay = new OptimisticOverlay();
    const { release } = overlay.apply("cabinet", removeDoc(DOC_A));
    overlay.apply("cabinet", removeDoc(DOC_B));

    expect(release()).toBe(true);
    expect(release()).toBe(false); // second release must not remove an unrelated patch

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

    const { release } = overlay.apply("cabinet", removeDoc(DOC_A));
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

  describe("predicate-based release (settleWhen / setBase)", () => {
    it("holds a patch until a fresh base satisfies its predicate", () => {
      const overlay = new OptimisticOverlay();
      // Optimistic add of DOC_C; releases once the base lists DOC_C (echo).
      const { settleWhen } = overlay.apply("cabinet", addDoc(DOC_C));
      settleWhen((base) => base.directories[ROOT_URI]?.entries.some((e) => e.uri === DOC_C) ?? false);

      // Base without DOC_C yet — patch must stay.
      overlay.setBase("cabinet", emptySnapshot);
      expect(overlay.patchCount("cabinet")).toBe(1);

      // Echo arrives: base now lists DOC_C — patch releases.
      overlay.setBase("cabinet", snapshotWith(DOC_C));
      expect(overlay.patchCount("cabinet")).toBe(0);
    });

    it("releases synchronously when the base already satisfies the predicate", () => {
      const overlay = new OptimisticOverlay();
      // Echo beat the success callback: base already reflects the delete.
      overlay.setBase("cabinet", snapshotWithout(DOC_A));

      const { settleWhen } = overlay.apply("cabinet", removeDoc(DOC_A));
      expect(overlay.patchCount("cabinet")).toBe(1);

      settleWhen((base) => !(base.directories[ROOT_URI]?.entries.some((e) => e.uri === DOC_A) ?? false));
      // Predicate true against the cached base — released without a new fire.
      expect(overlay.patchCount("cabinet")).toBe(0);
    });

    it("does not release patches that have no armed predicate", () => {
      const overlay = new OptimisticOverlay();
      overlay.apply("cabinet", removeDoc(DOC_A)); // never armed

      overlay.setBase("cabinet", snapshotWithout(DOC_A));
      expect(overlay.patchCount("cabinet")).toBe(1);
    });

    it("setBase with no patches is a no-op and does not throw", () => {
      const overlay = new OptimisticOverlay();
      expect(() => overlay.setBase("cabinet", emptySnapshot)).not.toThrow();
      expect(overlay.patchCount("cabinet")).toBe(0);
    });

    it("manual release after predicate release reports false", () => {
      const overlay = new OptimisticOverlay();
      const { release, settleWhen } = overlay.apply("cabinet", removeDoc(DOC_A));
      settleWhen((base) => !(base.directories[ROOT_URI]?.entries.some((e) => e.uri === DOC_A) ?? false));
      overlay.setBase("cabinet", snapshotWithout(DOC_A));
      expect(overlay.patchCount("cabinet")).toBe(0);

      // Fallback timeout fires later — must report it had nothing to do.
      expect(release()).toBe(false);
    });

    it("notifies subscribers when a predicate releases a patch", () => {
      const overlay = new OptimisticOverlay();
      const callback = vi.fn();
      const { settleWhen } = overlay.apply("cabinet", removeDoc(DOC_A));
      overlay.subscribe("cabinet", callback);
      settleWhen((base) => !(base.directories[ROOT_URI]?.entries.some((e) => e.uri === DOC_A) ?? false));

      overlay.setBase("cabinet", snapshotWithout(DOC_A));
      expect(callback).toHaveBeenCalled();
    });
  });
});
