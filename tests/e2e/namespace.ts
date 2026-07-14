// Actor namespaces: the single knob that lets two suite invocations run
// concurrently against one dev-env without contending on accounts, auth
// snapshots, or artifacts.
//
// `E2E_ACTOR_NS` unset (or empty) is the default population: the six checked-in
// fixture actors from dev-env/fixtures/actors.json and today's bare paths. A
// non-empty namespace derives its own six actors — same roles, same PDS
// placement, so the cross-PDS pairings the federation specs rely on survive —
// with handles and mnemonics derived from the namespace alone. Nothing is
// checked in per namespace and no manifest is written: derivation is the
// registry.
//
// Imported by fixtures.ts, auth.setup.ts, playwright.config.ts, pds-admin.ts
// and the federation tier, so it must stay free of @playwright/test (vitest
// collects the federation tests and would choke on the import).
import { entropyToMnemonic } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

export interface Actor {
  readonly name: string;
  readonly handle: string;
  readonly pds: string;
  readonly mnemonic: string;
  readonly password: string;
}

// Namespaces embed in handles and directory names, so the grammar is tight —
// and the length cap is not a matter of taste. The PDS rejects any handle over
// 29 characters ("InvalidHandle: Handle too long"), a namespaced handle is
// `<role>-<ns>.<pds>.test`, and the longest role is 5 characters ("alice"), so:
//
//   29 − len(".pds-a.test") − len("alice") − len("-")  =  29 − 11 − 5 − 1  =  12
//
// Exceed it and the failure lands mid-provisioning, as an opaque 400 from a PDS,
// which is precisely the kind of confusion this validates away.
const NAMESPACE_GRAMMAR = /^[a-z0-9-]{1,12}$/;

const fixturesPath = fileURLToPath(
  new URL("../../dev-env/fixtures/actors.json", import.meta.url),
);

/** The checked-in six. Roles and PDS placement for every namespace derive from these. */
export const DEFAULT_ACTORS: readonly Actor[] = JSON.parse(
  readFileSync(fixturesPath, "utf8"),
).actors;

/**
 * The namespace this process runs in — `""` for the default population.
 * Validated eagerly (at import, before any network activity) so a typo fails
 * with a clear message instead of a confusing handle-resolution error later.
 */
export function actorNamespace(): string {
  const raw = (process.env.E2E_ACTOR_NS ?? "").trim();
  if (raw === "") return "";
  if (!NAMESPACE_GRAMMAR.test(raw)) {
    throw new Error(
      `E2E_ACTOR_NS="${raw}" is not a valid actor namespace — ` +
        `expected ${NAMESPACE_GRAMMAR.source} (lowercase letters, digits, hyphens; 1–16 chars)`,
    );
  }
  return raw;
}

/** Same validation, for callers holding a namespace that did not come from the env. */
export function assertNamespace(ns: string): string {
  if (!NAMESPACE_GRAMMAR.test(ns)) {
    throw new Error(
      `"${ns}" is not a valid actor namespace — expected ${NAMESPACE_GRAMMAR.source}`,
    );
  }
  return ns;
}

/**
 * BIP-39 mnemonic for a namespaced actor: 32 bytes of SHA-256 over
 * `opake-e2e:<ns>:<role>` as entropy → 24 words. Deterministic from the
 * namespace, which is what makes a namespace's encryption identity stable
 * across provisionings and dev-env resets without checking anything in.
 */
export function deriveMnemonic(ns: string, role: string): string {
  assertNamespace(ns);
  const entropy = createHash("sha256").update(`opake-e2e:${ns}:${role}`).digest();
  return entropyToMnemonic(new Uint8Array(entropy), wordlist);
}

/** The six actors of a namespace: default set verbatim, or derived from `ns`. */
export function actorsFor(ns: string): readonly Actor[] {
  if (ns === "") return DEFAULT_ACTORS;
  assertNamespace(ns);
  return DEFAULT_ACTORS.map((actor) => ({
    name: actor.name,
    handle: `${actor.name}-${ns}.${actor.pds}.test`,
    pds: actor.pds,
    mnemonic: deriveMnemonic(ns, actor.name),
    password: actor.password,
  }));
}

export interface NamespacePaths {
  /** Persisted auth snapshots (one file per actor). */
  readonly authDir: string;
  /** Playwright `outputDir` — traces, screenshots, error contexts. */
  readonly outputDir: string;
  /** HTML reporter `outputFolder`. */
  readonly reportDir: string;
}

const testsDir = (segment: string): string =>
  fileURLToPath(new URL(`../${segment}`, import.meta.url));

/**
 * Where a run writes. Bare paths for the default population — the exact
 * locations used before namespaces existed — and a per-namespace subdirectory
 * otherwise, so concurrent runs never write into each other's evidence.
 */
export function nsPaths(ns: string = actorNamespace()): NamespacePaths {
  const suffix = ns === "" ? "" : `/${assertNamespace(ns)}`;
  return {
    authDir: fileURLToPath(new URL(`./.auth${suffix}`, import.meta.url)),
    outputDir: testsDir(`test-results${suffix}`),
    reportDir: testsDir(`playwright-report${suffix}`),
  };
}
