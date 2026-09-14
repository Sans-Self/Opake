// Workspace-identity regression net: every operation on a CHURNED workspace —
// one whose keyring chain has superseded ≥2 records past genesis, so head ≠
// genesis — still keys on the genesis URI. This is the property class that
// shipped two real bugs: the head-URI-as-workspace-id defect (a workspace-scoped
// indexer call carrying a head URI resolves to no `chain_heads` row, fix
// `e607210` — it was rejected as a non-member 403 then; the same call now
// answers `workspace_not_indexed` and burns the client's visibility window to a
// timeout, which is why the wire contract cannot catch it and this test must)
// and the genesis-anchor AEAD binding (a group-key unwrap reconstructing its
// context from the head fails to decrypt, fix `2c9b32d`). Both are silent on a
// fresh workspace because head equals genesis until the first supersede — so
// the whole point is to move the head first.
//
// Driven through the opake CLI against the dockerized dev-env and asserted
// through the indexer's view, exactly as leave-smoke does. The CLI has no
// keepers; `workspace ls` is a direct indexer query, so what it prints is what
// the indexer resolved. This tier drives the CLI, not the WASM bindings, so it
// cannot honestly cite the WASM-boundary requirement — that stays for a web
// e2e (see COVERAGE_ROADMAP batch 2).
//
// Only runs under OPAKE_TEST_ENV=devenv (`just e2e-federation`).

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { testEnv } from "../../helpers/pds.js";
import {
  actorByName,
  actorOnPds,
  cli,
  cliApprovingUnverified,
  downloadAsMember,
  headUri,
  login,
  memberCount,
  parseCreateUri,
  pollUntil,
  rotationCount,
  startCli,
  stackIsUp,
  stopCli,
  uploadTextToWorkspace,
  workspaceListed,
} from "../../helpers/devenv.js";

// Three actors on three DIFFERENT PDSes so churn crosses federation boundaries
// and the post-churn member is genuinely foreign to the owner. Foreign
// identities are addressed BY DID (handle→DID for a foreign actor is the known
// dev-env gap; by-DID resolution is fully local via the PLC).
const OWNER = actorOnPds("pds-a").name; // alice
const MEMBER = actorOnPds("pds-b").name; // carol
const LATE = actorByName("eve").name; // pds-c, added AFTER the head moves past genesis

// Fixture identities accumulate workspaces across runs, so tests never assert
// on totals — each names its workspace uniquely and asserts on that one.
// eslint-disable-next-line functional/no-let
let seq = 0;
const uniqueName = (tag: string): string => `churn-${tag}-${Date.now()}-${seq++}`;

// eslint-disable-next-line functional/no-let
let memberDid = "";
// eslint-disable-next-line functional/no-let
let lateDid = "";

describe.skipIf(testEnv() !== "devenv")("workspace-identity churn", () => {
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
    // spec:workspace-identity § Workspace-scoped indexer calls pass genesis
    // spec:workspace-identity § Genesis URI is the workspace identity
    "add-member on a superseded chain succeeds (genesis-keyed), and the head keeps moving while the workspace persists",
    async () => {
      const ws = uniqueName("add");
      const created = await cli(OWNER, ["workspace", "create", ws]);
      expect(created.code).toBe(0);
      const genesisUri = parseCreateUri(created.stdout);

      // The client absorbs the chain-head visibility gap, but a CLI mutation
      // names its workspace, and name→workspace resolution runs through the
      // indexer's workspace list (`discover_member_workspaces`), which is NOT
      // inside that retry — a fresh workspace is simply "no keyring named X"
      // until it is listed. So the poll stays, and it is a workaround for that
      // gap, not for the membership check the client now handles. Removing it
      // once name resolution retries too is tracked with the finding; the
      // no-poll contract itself is pinned in visibility-contract.test.ts.
      expect(await pollUntil(() => workspaceListed(OWNER, ws))).toBe(true);

      // First supersede: the head record is now a different URI from genesis.
      const added1 = await cliApprovingUnverified(OWNER, ["workspace", "add-member", ws, memberDid]);
      expect(added1.code).toBe(0);
      expect(
        await pollUntil(async () => (await memberCount(OWNER, ws)) === 2),
      ).toBe(true);
      const headAfterFirst = await headUri(OWNER, ws);
      expect(headAfterFirst).not.toBeNull();
      expect(headAfterFirst).not.toBe(genesisUri); // head has moved past genesis

      // Second supersede on a workspace whose head ≠ genesis. THIS is the
      // shipped bug's exact shape: the add's chain-head lookup and membership
      // check must carry the genesis URI. A head-keyed call resolves to no
      // `chain_heads` row, is answered `workspace_not_indexed`, and retries to
      // a visibility timeout; success here is the genesis-keyed path.
      const added2 = await cliApprovingUnverified(OWNER, ["workspace", "add-member", ws, lateDid]);
      expect(added2.code).toBe(0);
      expect(
        await pollUntil(async () => (await memberCount(OWNER, ws)) === 3),
      ).toBe(true);

      const headAfterSecond = await headUri(OWNER, ws);
      expect(headAfterSecond).not.toBeNull();
      // The head URI is not the workspace identity: it churned again, yet the
      // workspace is still the same single named entity in the indexer's view.
      expect(headAfterSecond).not.toBe(headAfterFirst);
      expect(headAfterSecond).not.toBe(genesisUri);
      expect(await workspaceListed(OWNER, ws)).toBe(true);
    },
    180_000,
  );

  it(
    // spec:workspace-identity § Workspace-scoped indexer calls pass genesis
    // spec:workspace-identity § Genesis URI is the workspace identity
    "remove-member on a superseded chain resolves to genesis, rotates the key, and leaves the workspace live",
    async () => {
      const ws = uniqueName("remove");
      const created = await cli(OWNER, ["workspace", "create", ws]);
      expect(created.code).toBe(0);
      const genesisUri = parseCreateUri(created.stdout);
      expect(await pollUntil(() => workspaceListed(OWNER, ws))).toBe(true);

      // Churn the head two supersedes past genesis with two adds.
      expect(
        (await cliApprovingUnverified(OWNER, ["workspace", "add-member", ws, memberDid])).code,
      ).toBe(0);
      expect(
        await pollUntil(async () => (await memberCount(OWNER, ws)) === 2),
      ).toBe(true);
      expect(
        (await cliApprovingUnverified(OWNER, ["workspace", "add-member", ws, lateDid])).code,
      ).toBe(0);
      expect(
        await pollUntil(async () => (await memberCount(OWNER, ws)) === 3),
      ).toBe(true);

      // add-member does not rotate the group key; the counter is still 0 here.
      expect(await rotationCount(OWNER, ws)).toBe(0);

      // Remove on a churned workspace: a rotation-bearing mutation that must
      // resolve to genesis for both the chain-head lookup and the new wrap
      // context. Distinct from the add path — proves the property for the
      // remove verb too.
      const removed = await cli(OWNER, ["workspace", "remove-member", ws, memberDid, "-y"]);
      expect(removed.code).toBe(0);
      expect(
        await pollUntil(async () => (await memberCount(OWNER, ws)) === 2),
      ).toBe(true);

      // The removal rotated the group key (0 → 1) and the workspace — keyed on
      // the unchanged genesis URI — is still live.
      expect(await pollUntil(async () => (await rotationCount(OWNER, ws)) === 1)).toBe(true);
      expect(await workspaceListed(OWNER, ws)).toBe(true);
      // Genesis stays fixed while the head churned across three supersedes.
      expect(await headUri(OWNER, ws)).not.toBe(genesisUri);
    },
    180_000,
  );

  it(
    // spec:workspace-identity § Workspace-scoped indexer calls pass genesis
    // spec:workspace-identity § Genesis URI is the workspace identity
    // spec:workspace-membership § Removal rotates the group key; leave does not
    "leave on a superseded chain resolves to genesis and the workspace outlives the leaver",
    async () => {
      const ws = uniqueName("leave");
      const created = await cli(OWNER, ["workspace", "create", ws]);
      expect(created.code).toBe(0);
      const genesisUri = parseCreateUri(created.stdout);
      expect(await pollUntil(() => workspaceListed(OWNER, ws))).toBe(true);

      // Two supersedes. The leaver joins at the SECOND one, so they appear in
      // neither the genesis member list nor the first head's — the record they
      // supersede when leaving is one they never authored and never saw
      // created. Their leave has to find the workspace by its genesis identity;
      // nothing they hold locally points at it.
      expect(
        (await cliApprovingUnverified(OWNER, ["workspace", "add-member", ws, memberDid])).code,
      ).toBe(0);
      expect(
        await pollUntil(async () => (await memberCount(OWNER, ws)) === 2),
      ).toBe(true);
      expect(
        (await cliApprovingUnverified(OWNER, ["workspace", "add-member", ws, lateDid])).code,
      ).toBe(0);
      expect(
        await pollUntil(async () => (await memberCount(OWNER, ws)) === 3),
      ).toBe(true);
      expect(await pollUntil(() => workspaceListed(LATE, ws))).toBe(true);

      const headBeforeLeave = await headUri(OWNER, ws);
      expect(headBeforeLeave).not.toBe(genesisUri);

      // Pure self-removal, authored on the leaver's own PDS against a chain
      // head hosted on the owner's. A head-keyed chain-head lookup here would
      // resolve to no `chain_heads` row, be answered `workspace_not_indexed`,
      // and burn the visibility window to a timeout rather than exit 0.
      const left = await cli(LATE, ["workspace", "leave", ws, "-y"]);
      expect(left.code, `leave on a churned chain failed: ${left.stderr}`).toBe(0);

      // The leaver's own view drops it; the workspace itself is untouched —
      // same identity, two remaining members, and no rotation (leave doesn't
      // rotate the group key; only removal does).
      expect(await pollUntil(async () => !(await workspaceListed(LATE, ws)))).toBe(true);
      expect(
        await pollUntil(async () => (await memberCount(OWNER, ws)) === 2),
      ).toBe(true);
      expect(await rotationCount(OWNER, ws)).toBe(0);
      expect(await workspaceListed(OWNER, ws)).toBe(true);
      expect(await workspaceListed(MEMBER, ws)).toBe(true);

      // The leave minted yet another head. Genesis never moved.
      const headAfterLeave = await headUri(OWNER, ws);
      expect(headAfterLeave).not.toBe(headBeforeLeave);
      expect(headAfterLeave).not.toBe(genesisUri);
    },
    180_000,
  );

  it(
    // spec:workspace-identity § Membership authority is the live chain head
    // spec:workspace-identity § Group-key wraps are AEAD-bound to genesis
    "a member added after the head moved past genesis downloads a document uploaded before they joined",
    async () => {
      const ws = uniqueName("history");
      const created = await cli(OWNER, ["workspace", "create", ws]);
      expect(created.code).toBe(0);
      expect(await pollUntil(() => workspaceListed(OWNER, ws))).toBe(true);

      // Upload BEFORE anyone else joins. The genesis member list — frozen at
      // creation — does not name the late member, so a genesis-gated download
      // would refuse them.
      const marker = `pre-membership-${Date.now()}-${seq++}`;
      const docUri = await uploadTextToWorkspace(OWNER, ws, "history.txt", marker);

      // Two supersedes: the late member joins only at the second, well past
      // genesis, so membership authority must be read at the live head.
      expect(
        (await cliApprovingUnverified(OWNER, ["workspace", "add-member", ws, memberDid])).code,
      ).toBe(0);
      expect(
        await pollUntil(async () => (await memberCount(OWNER, ws)) === 2),
      ).toBe(true);
      expect(
        (await cliApprovingUnverified(OWNER, ["workspace", "add-member", ws, lateDid])).code,
      ).toBe(0);
      expect(
        await pollUntil(async () => (await memberCount(OWNER, ws)) === 3),
      ).toBe(true);

      // The late member sees the workspace in their own indexer view.
      expect(await pollUntil(() => workspaceListed(LATE, ws))).toBe(true);

      // First-time cross-PDS download: the late member resolves the workspace
      // at the live head (finding themselves in the member list) and unwraps
      // their group key using the GENESIS URI as AEAD context. Success proves
      // both: authority is the live head (they were absent from genesis), and
      // the wrap/unwrap round-trip is genesis-anchored (a head context would
      // fail the AEAD tag and the download would error rather than return the
      // plaintext).
      const got = await pollUntil(async () => {
        const res = await downloadAsMember(LATE, docUri);
        return res.code === 0 && res.stdout === marker;
      });
      expect(got).toBe(true);
    },
    180_000,
  );
});
