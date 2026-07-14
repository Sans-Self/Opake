// Actor-namespace derivation: the whole registry of a namespaced actor is the
// namespace name. Handles, PDS placement, and mnemonics are pure functions of
// (namespace, role) — nothing is checked in per namespace, nothing is written to
// a manifest, and provisioning the same namespace against a reset dev-env
// therefore rebuilds the same encryption identities.
//
// The mnemonic vectors below are pinned deliberately: a change to the derivation
// scheme silently re-keys every namespaced actor (their published X25519 key
// changes, so nothing they encrypted before remains readable). If this test
// fails, the derivation changed — that is a decision, not a nuisance.

import { describe, it, expect } from "vitest";
import {
  DEFAULT_ACTORS,
  actorsFor,
  assertNamespace,
  deriveMnemonic,
  nsPaths,
} from "../e2e/namespace.js";

describe("actor namespaces", () => {
  it(
    // spec:dev-env § Deterministic actor fixtures
    "derives a stable mnemonic from (namespace, role)",
    () => {
      expect(deriveMnemonic("alpha", "alice")).toBe(
        "ability fuel ice arrange weasel danger spider flame cushion load near amateur " +
          "grain shuffle island flame twenty arch monitor bench rotate buddy execute electric",
      );
      expect(deriveMnemonic("alpha", "frank")).toBe(
        "dizzy sweet casino task muscle siren wealth fine cinnamon cement obtain suffer " +
          "knock leaf matrix follow lottery dynamic keen diesel swift imitate people law",
      );
      expect(deriveMnemonic("alpha", "alice").split(/\s+/)).toHaveLength(24);
    },
  );

  it(
    // spec:dev-env § Deterministic actor fixtures
    "gives every namespace and every role a distinct mnemonic",
    () => {
      expect(deriveMnemonic("alpha", "alice")).not.toBe(deriveMnemonic("beta", "alice"));
      expect(deriveMnemonic("alpha", "alice")).not.toBe(deriveMnemonic("alpha", "bob"));
    },
  );

  it(
    // spec:dev-env § Deterministic actor fixtures
    "mirrors the checked-in roles and their PDS placement",
    () => {
      const actors = actorsFor("alpha");
      expect(actors.map((a) => a.name)).toEqual(DEFAULT_ACTORS.map((a) => a.name));
      expect(actors.map((a) => a.pds)).toEqual(DEFAULT_ACTORS.map((a) => a.pds));
      expect(actors.map((a) => a.handle)).toEqual([
        "alice-alpha.pds-a.test",
        "bob-alpha.pds-a.test",
        "carol-alpha.pds-b.test",
        "dave-alpha.pds-b.test",
        "eve-alpha.pds-c.test",
        "frank-alpha.pds-c.test",
      ]);
    },
  );

  it(
    // spec:dev-env § Deterministic actor fixtures
    "leaves the default population verbatim when no namespace is set",
    () => {
      expect(actorsFor("")).toBe(DEFAULT_ACTORS);
      const bare = nsPaths("");
      expect(bare.authDir.endsWith("/e2e/.auth")).toBe(true);
      expect(bare.outputDir.endsWith("/tests/test-results")).toBe(true);
      expect(bare.reportDir.endsWith("/tests/playwright-report")).toBe(true);
    },
  );

  it(
    // spec:e2e-testing § Parallel workers do not share mutable state
    "partitions every artifact directory per namespace",
    () => {
      const alpha = nsPaths("alpha");
      const beta = nsPaths("beta");
      expect(alpha.authDir.endsWith("/e2e/.auth/alpha")).toBe(true);
      expect(alpha.outputDir.endsWith("/tests/test-results/alpha")).toBe(true);
      expect(alpha.reportDir.endsWith("/tests/playwright-report/alpha")).toBe(true);
      for (const key of ["authDir", "outputDir", "reportDir"] as const) {
        expect(alpha[key]).not.toBe(beta[key]);
      }
    },
  );

  it(
    // spec:dev-env § Deterministic actor fixtures
    "rejects a namespace that cannot embed in a handle or a directory name",
    () => {
      for (const bad of ["Alpha", "a b", "alpha.beta", "alpha_beta", "a".repeat(13), ""]) {
        expect(() => assertNamespace(bad)).toThrow(/not a valid actor namespace/);
      }
      expect(assertNamespace("a-9")).toBe("a-9");
    },
  );

  it(
    // spec:dev-env § Deterministic actor fixtures
    "keeps every derived handle inside the PDS's 29-character limit",
    () => {
      // The longest handle a legal namespace can produce. Over the limit the PDS
      // rejects the registration with an opaque InvalidHandle, mid-provisioning.
      const longest = actorsFor("a".repeat(12))
        .map((a) => a.handle.length)
        .reduce((max, len) => Math.max(max, len), 0);
      expect(longest).toBeLessThanOrEqual(29);
    },
  );
});
