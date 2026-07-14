// Namespaced actor populations against the real dev-env: provisioning is
// deterministic, isolated, and individually disposable.
//
// These drive the PDSes directly (registration, key records, account deletion)
// rather than through the CLI — the thing under test is the harness's actor
// lifecycle, not an opake command. They live in the federation tier because
// they need the dockerized stack, and because a namespace's actors span all
// three PDSes exactly as the default six do.
//
// Every test provisions its own namespace and deprovisions it afterwards, so
// the suite leaves the dev-env as it found it and never depends on a reset.
//
// Only runs under OPAKE_TEST_ENV=devenv (`just e2e-federation`).

import { describe, it, expect, afterAll } from "vitest";
import { testEnv } from "../../helpers/pds.js";
import { actorsFor, DEFAULT_ACTORS } from "../../e2e/namespace.js";
import {
  deprovisionNamespace,
  provisionNamespace,
  publishedEncryptionKeys,
  resolveHandle,
} from "../../e2e/pds-admin.js";

// Namespaces are cheap and disposable, but two tests in one file must not
// collide, and a crashed run must not poison the next one — so each test owns a
// namespace and tears it down in `afterAll` regardless of outcome.
const DETERMINISM_NS = "t-det";
const KEEP_NS = "t-keep";
const DROP_NS = "t-drop";

describe.skipIf(testEnv() !== "devenv")("actor namespaces", () => {
  afterAll(async () => {
    for (const ns of [DETERMINISM_NS, KEEP_NS, DROP_NS]) {
      await deprovisionNamespace(ns);
    }
  }, 120_000);

  it(
    // spec:dev-env § Deterministic actor fixtures
    "a namespace provisioned twice yields the same actors and the same published keys",
    async () => {
      const actors = actorsFor(DETERMINISM_NS);
      const defaultKeysBefore = await publishedEncryptionKeys(DEFAULT_ACTORS[0]!);

      await provisionNamespace(DETERMINISM_NS);
      const first = await Promise.all(actors.map(publishedEncryptionKeys));
      expect(first.every((keys) => keys !== null)).toBe(true);

      // Wipe the accounts and provision the same namespace again. Nothing was
      // written down between the two: the handles, PDS placement, and mnemonics
      // are re-derived from the namespace name alone — the same rebuild a reset
      // dev-env would force.
      await deprovisionNamespace(DETERMINISM_NS);
      expect(await resolveHandle(actors[0]!)).toBeNull();

      await provisionNamespace(DETERMINISM_NS);
      const second = await Promise.all(actors.map(publishedEncryptionKeys));

      // Same handles, same PDS placement, same encryption identities. The DIDs
      // are new — a deleted account's DID does not come back — which is exactly
      // why the guarantee is stated over keys and not over identifiers.
      expect(second).toEqual(first);
      expect(actors.map((a) => a.pds)).toEqual(DEFAULT_ACTORS.map((a) => a.pds));

      // …and the checked-in population never moved.
      expect(await publishedEncryptionKeys(DEFAULT_ACTORS[0]!)).toEqual(defaultKeysBefore);
    },
    600_000,
  );

  it(
    // spec:dev-env § Deterministic actor fixtures
    "deprovisioning removes exactly one namespace",
    async () => {
      await provisionNamespace(KEEP_NS);
      await provisionNamespace(DROP_NS);

      const keep = actorsFor(KEEP_NS);
      const drop = actorsFor(DROP_NS);

      // The provisioned actors carry state, not just accounts: bootstrap seeds
      // each one's cabinet with a document (root directory + document record),
      // so this is a populated namespace being disposed of, not an empty shell.
      const keepKeys = await Promise.all(keep.map(publishedEncryptionKeys));
      expect(keepKeys.every((k) => k !== null)).toBe(true);
      expect(await publishedEncryptionKeys(drop[0]!)).not.toBeNull();
      const defaultKeys = await Promise.all(DEFAULT_ACTORS.map(publishedEncryptionKeys));

      await deprovisionNamespace(DROP_NS);

      // The dropped namespace is gone — accounts unresolvable, records with them.
      const dropped = await Promise.all(drop.map(resolveHandle));
      expect(dropped).toEqual(drop.map(() => null));
      expect(await publishedEncryptionKeys(drop[0]!)).toBeNull();

      // Its neighbours are untouched: the other namespace and the default six
      // keep their accounts and their published keys.
      expect(await Promise.all(keep.map(publishedEncryptionKeys))).toEqual(keepKeys);
      expect(await Promise.all(DEFAULT_ACTORS.map(publishedEncryptionKeys))).toEqual(defaultKeys);
    },
    600_000,
  );
});
