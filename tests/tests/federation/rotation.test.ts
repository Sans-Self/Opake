// Key-rotation lifecycle across federation boundaries, driven through the
// opake CLI against the dockerized dev-env and asserted through the indexer's
// view — the same tier as identity-churn and leave-smoke.
//
// What this tier can and cannot show: the CLI has no keepers, so the
// "live projection adopts a rotation without reload" requirement
// (`spec:key-rotation § Live projections adopt a rotation completely`) is a
// WASM/keeper property and is pinned by the tree-keeper unit regression
// (`rotation_event_keeps_names_readable_across_rotation`), not here — every
// CLI download re-resolves from the indexer, so there is no live projection to
// keep. What this tier CAN show is the protocol-level truth: forward secrecy
// against a removed member, a post-rotation joiner reading pre-rotation
// documents, and deep-history reads through many rotations.
//
// Three actors on three PDSes so membership churn crosses federation
// boundaries. Foreign members are addressed by DID (the dev-env handle→DID gap
// for foreign actors; by-DID resolution is fully local via the PLC).
//
// Only runs under OPAKE_TEST_ENV=devenv (`just e2e-federation`).

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { testEnv } from "../../helpers/pds.js";
import {
  actorByName,
  actorOnPds,
  cli,
  downloadAsMember,
  login,
  memberCount,
  pollUntil,
  rotationCount,
  startCli,
  stackIsUp,
  stopCli,
  uploadTextToWorkspace,
  workspaceListed,
} from "../../helpers/devenv.js";

const OWNER = actorOnPds("pds-a").name; // alice — the constant manager
const MEMBER = actorOnPds("pds-b").name; // carol — removed to force a rotation
const LATE = actorByName("eve").name; // pds-c — joins AFTER a rotation

// eslint-disable-next-line functional/no-let
let seq = 0;
const uniqueName = (tag: string): string => `rot-${tag}-${Date.now()}-${seq++}`;

// eslint-disable-next-line functional/no-let
let memberDid = "";
// eslint-disable-next-line functional/no-let
let lateDid = "";

describe.skipIf(testEnv() !== "devenv")("key rotation lifecycle", () => {
  beforeAll(async () => {
    if (!(await stackIsUp())) {
      throw new Error(
        "dev-env stack is not up — run `just dev-env-up` before `just e2e-federation`",
      );
    }
    await startCli();
    await login(OWNER);
    memberDid = await login(MEMBER);
    lateDid = await login(LATE);
  }, 180_000);

  afterAll(async () => {
    await stopCli();
  });

  it(
    // A removed member cannot read content created after the rotation their
    // removal triggered — forward secrecy is complete the moment the supersede
    // lands, no follow-up sweep required.
    // spec:key-rotation § The rotation event is synchronous and self-sufficient
    // spec:workspace-membership § Removal rotates the group key; leave does not
    "removed member cannot decrypt a document uploaded after the rotation",
    async () => {
      const ws = uniqueName("fwdsec");
      const created = await cli(OWNER, ["workspace", "create", ws]);
      expect(created.code).toBe(0);
      expect(await pollUntil(() => workspaceListed(OWNER, ws))).toBe(true);

      expect((await cli(OWNER, ["workspace", "add-member", ws, memberDid])).code).toBe(0);
      expect(await pollUntil(async () => (await memberCount(OWNER, ws)) === 2)).toBe(true);

      // Remove the member → rotates the group key (0 → 1).
      expect((await cli(OWNER, ["workspace", "remove-member", ws, memberDid, "-y"])).code).toBe(0);
      expect(await pollUntil(async () => (await rotationCount(OWNER, ws)) === 1)).toBe(true);

      // Upload AFTER the rotation — wrapped under the rotation-1 key.
      const marker = `post-rotation-${Date.now()}-${seq++}`;
      const docUri = await uploadTextToWorkspace(OWNER, ws, "secret.txt", marker);

      // A remaining member (the owner) reads it.
      expect(
        await pollUntil(async () => {
          const res = await downloadAsMember(OWNER, docUri);
          return res.code === 0 && res.stdout === marker;
        }),
      ).toBe(true);

      // The removed member cannot: they are absent from the rotation-1 member
      // list and the rotation-1 key was never wrapped to them.
      const denied = await downloadAsMember(MEMBER, docUri);
      expect(denied.code).not.toBe(0);
      expect(denied.stdout).not.toBe(marker);
    },
    180_000,
  );

  it(
    // A member admitted AFTER a rotation reads a document written under the
    // prior rotation — the admitting supersede wraps the retained historical
    // key to the joiner. Without that, the joiner has only the current key and
    // the pre-rotation document is unreadable to them.
    // spec:key-rotation § New members can read the full history they are admitted to
    "post-rotation joiner reads a document written before the rotation",
    async () => {
      const ws = uniqueName("joiner");
      const created = await cli(OWNER, ["workspace", "create", ws]);
      expect(created.code).toBe(0);
      expect(await pollUntil(() => workspaceListed(OWNER, ws))).toBe(true);

      // A transient member exists so the removal has someone to rotate out.
      expect((await cli(OWNER, ["workspace", "add-member", ws, memberDid])).code).toBe(0);
      expect(await pollUntil(async () => (await memberCount(OWNER, ws)) === 2)).toBe(true);

      // Upload UNDER rotation 0.
      const marker = `pre-rotation-${Date.now()}-${seq++}`;
      const docUri = await uploadTextToWorkspace(OWNER, ws, "history.txt", marker);

      // Remove the transient member → rotation 0 → 1, archiving rotation 0 into
      // keyHistory.
      expect((await cli(OWNER, ["workspace", "remove-member", ws, memberDid, "-y"])).code).toBe(0);
      expect(await pollUntil(async () => (await rotationCount(OWNER, ws)) === 1)).toBe(true);

      // Admit the late joiner AFTER the rotation. The admitting supersede wraps
      // both the current key and the retained rotation-0 key to them.
      expect((await cli(OWNER, ["workspace", "add-member", ws, lateDid])).code).toBe(0);
      expect(await pollUntil(async () => (await memberCount(OWNER, ws)) === 2)).toBe(true);
      expect(await pollUntil(() => workspaceListed(LATE, ws))).toBe(true);

      // The joiner downloads the pre-rotation document — resolving the
      // rotation-0 key from their own keyHistory wrap.
      expect(
        await pollUntil(async () => {
          const res = await downloadAsMember(LATE, docUri);
          return res.code === 0 && res.stdout === marker;
        }),
      ).toBe(true);
    },
    180_000,
  );

  it(
    // A continuous member reads a rotation-0 document after the keyring has
    // rotated many times — the read walks the full key history and succeeds.
    // History depth is a performance cost, never a correctness cliff.
    // spec:key-rotation § Unbounded key history is the accepted cost of unswept workspaces
    "deep history: rotation-0 document stays readable after many rotations",
    async () => {
      const ws = uniqueName("deep");
      const created = await cli(OWNER, ["workspace", "create", ws]);
      expect(created.code).toBe(0);
      expect(await pollUntil(() => workspaceListed(OWNER, ws))).toBe(true);

      // Upload at rotation 0 before any churn.
      const marker = `oldest-${Date.now()}-${seq++}`;
      const docUri = await uploadTextToWorkspace(OWNER, ws, "oldest.txt", marker);

      // Three add+remove cycles → rotation climbs to 3. The owner is the
      // constant manager and retains every historical key.
      const churn = [memberDid, lateDid, memberDid];
      for (const [i, did] of churn.entries()) {
        expect((await cli(OWNER, ["workspace", "add-member", ws, did])).code).toBe(0);
        expect(await pollUntil(async () => (await memberCount(OWNER, ws)) === 2)).toBe(true);
        expect((await cli(OWNER, ["workspace", "remove-member", ws, did, "-y"])).code).toBe(0);
        expect(await pollUntil(async () => (await rotationCount(OWNER, ws)) === i + 1)).toBe(true);
      }
      expect(await rotationCount(OWNER, ws)).toBe(3);

      // The oldest document still decrypts — the read resolves through the full
      // history to the rotation-0 key.
      expect(
        await pollUntil(async () => {
          const res = await downloadAsMember(OWNER, docUri);
          return res.code === 0 && res.stdout === marker;
        }),
      ).toBe(true);
    },
    240_000,
  );
});
