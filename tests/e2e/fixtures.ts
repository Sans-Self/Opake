// Shared e2e fixtures: the browser-side blockade, per-worker actor assignment,
// storageState wiring, and the spec-citation helper.
import { test as base, expect, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

export interface Actor {
  readonly name: string;
  readonly handle: string;
  readonly pds: string;
  readonly mnemonic: string;
  readonly password: string;
}

const fixturesPath = fileURLToPath(
  new URL("../../dev-env/fixtures/actors.json", import.meta.url),
);
export const ACTORS: readonly Actor[] = JSON.parse(
  readFileSync(fixturesPath, "utf8"),
).actors;

export const authFile = (actor: string): string =>
  fileURLToPath(new URL(`./.auth/${actor}.json`, import.meta.url));

// A request is allowed iff it stays on the host loopback or the dev-env's
// *.test space (Caddy). Anything else is a hermeticity escape (e.g. the WASM
// resolver falling back to plc.directory) and must fail the test loudly.
function isLocal(urlStr: string): boolean {
  try {
    const u = new URL(urlStr);
    if (u.protocol === "data:" || u.protocol === "blob:") return true;
    const h = u.hostname;
    return (
      h === "localhost" ||
      h === "127.0.0.1" ||
      h === "[::1]" ||
      h === "::1" ||
      h.endsWith(".test")
    );
  } catch {
    return true; // relative/opaque — same-origin, allowed
  }
}

/** Install the blockade on a page; returns a function that asserts no escapes. */
export async function installBlockade(page: Page): Promise<() => void> {
  const escapes: string[] = [];
  await page.route("**/*", (route) => {
    const url = route.request().url();
    if (isLocal(url)) return route.continue();
    escapes.push(url);
    return route.abort("blockedbyclient");
  });
  return () => {
    if (escapes.length > 0) {
      throw new Error(
        `browser-side hermeticity escape — non-local request(s): ${[...new Set(escapes)].join(", ")}`,
      );
    }
  };
}

/** Base test: blockade only. Used by the setup project (which has no session yet). */
export const blockadeTest = base.extend<{ assertNoEscape: void }>({
  assertNoEscape: [
    async ({ page }, use) => {
      const assert = await installBlockade(page);
      await use();
      assert();
    },
    { auto: true },
  ],
});

/** Authenticated test: blockade + a per-worker actor + that actor's storageState. */
export const test = blockadeTest.extend<
  { actor: Actor },
  { workerActor: Actor }
>({
  // One actor per worker → disjoint state across parallel workers.
  workerActor: [
    async ({}, use, workerInfo) => {
      const actor = ACTORS[workerInfo.workerIndex % ACTORS.length]!;
      await use(actor);
    },
    { scope: "worker" },
  ],
  actor: async ({ workerActor }, use) => {
    await use(workerActor);
  },
  storageState: async ({ workerActor }, use) => {
    await use(authFile(workerActor.name));
  },
});

export { expect };

/**
 * Spec-citation tag. Embed in a test title so `just spec-lint` resolves it
 * against openspec/specs/. Grammar requires the citation in double quotes or
 * backticks — this returns a plain string you interpolate into the title.
 *
 *   test(`restores session ${cite(
 *     "wasm-security-boundary",
 *     "Session persistence crosses as an opaque serialized value",
 *   )}`, …)
 */
export const cite = (capability: string, requirement: string): string =>
  `spec:${capability} § ${requirement}`;
