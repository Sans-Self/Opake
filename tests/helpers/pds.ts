// Fake PDS lifecycle for e2e tests.

import { createFakePds, type FakePds, type Account } from "fake-pds";
import { actorNamespace, actorsFor } from "../e2e/namespace.js";

// Which backing network the CLI tier runs against. The default is the
// in-process fake-pds (fast, hermetic, no docker). `OPAKE_TEST_ENV=devenv`
// selects the dockerized local atproto network under dev-env/ — real PLC,
// three federated PDSes, relay, jetstream, and the indexer — so the
// federation suite can exercise cross-PDS membership and indexer-observed
// outcomes that fake-pds cannot model. Nothing below the fake path reads
// this; it only gates the federation suite and its resolution helpers.
export type TestEnv = "fake" | "devenv";

export function testEnv(): TestEnv {
  return process.env.OPAKE_TEST_ENV === "devenv" ? "devenv" : "fake";
}

export interface DevenvActor {
  readonly name: string;
  readonly handle: string;
  readonly pds: string;
  readonly mnemonic: string;
  readonly password: string;
}

// Fixture actors resolve through the same namespace helper the web harness uses
// (tests/e2e/namespace.ts), which reads dev-env/fixtures/actors.json for the
// default population and derives the six actors of E2E_ACTOR_NS otherwise — so
// a federation run scoped to a namespace drives that namespace's actors, and
// there is no second copy of the fixture set to drift. Only meaningful in
// devenv mode.
export function devenvActors(): readonly DevenvActor[] {
  return actorsFor(actorNamespace());
}

export const TEST_ACCOUNTS: readonly Account[] = [
  { did: "did:plc:alice", handle: "alice.test" },
  { did: "did:plc:bob", handle: "bob.test", password: "bobsecret" },
  { did: "did:plc:charlie", handle: "charlie.test" },
  { did: "did:plc:dave", handle: "dave.test" },
  { did: "did:plc:eve", handle: "eve.test" },
  { did: "did:plc:frank", handle: "frank.test" },
];

// eslint-disable-next-line functional/no-let
let instance: FakePds | null = null;

export async function startPds(): Promise<FakePds> {
  instance = await createFakePds({ accounts: TEST_ACCOUNTS });
  return instance;
}

export function getPds(): FakePds {
  if (!instance) throw new Error("PDS not started — call startPds() first");
  return instance;
}

export function resetPds(): void {
  getPds().reset();
}

export async function stopPds(): Promise<void> {
  if (instance) {
    await instance.close();
    instance = null;
  }
}
