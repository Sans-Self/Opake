import { expect, test, ACTORS } from "../fixtures";
import { actorNamespace } from "../namespace";
import { beginRealPdsIdentityGrant } from "../oauth-dpop-fixture";

test.skip(actorNamespace() === "", "OAuth revocation uses a disposable actor namespace");

test("a real PDS revocation rejects a freshly DPoP-proved identity read", async ({ browser }) => {
  test.setTimeout(90_000);
  const actor = ACTORS.find((candidate) => candidate.name === "alice");
  if (!actor) throw new Error("missing alice fixture");
  const context = await browser.newContext({ ignoreHTTPSErrors: true });
  const page = await context.newPage();
  try {
    const grant = await beginRealPdsIdentityGrant(page, actor);
    expect(await grant.protectedRead()).toBe(200);
    await grant.revokeWithProductionCore();
    expect(await grant.protectedRead()).toBe(401);
  } finally {
    await context.close();
  }
});
