// Fake PDS lifecycle for e2e tests.

import { createFakePds, type FakePds, type Account } from "fake-pds";

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
