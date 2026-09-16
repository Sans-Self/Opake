// Materialize the fixture manifest's verified actor through the same holder
// used by a person: publish the signed public-key record, request a narrow
// OAuth identity grant, submit the owner code to the stock PDS signer, and
// wait for the authoritative PLC `#opake` method. The method is deliberately
// retained for the namespace; federation tests consume the durable fixture.
import { blockadeTest as test, ACTORS } from "./fixtures";
import { ensureVerifiedActor } from "./verification-helpers";

const verifiedActors = ACTORS.filter((actor) => actor.verified);

test("materialize verified fixture actors", async ({ browser }) => {
  test.setTimeout(180_000);
  if (verifiedActors.length !== 1 || verifiedActors[0]?.name !== "frank") {
    throw new Error("fixture manifest must designate exactly Frank as the verified actor");
  }

  const verified = await ensureVerifiedActor(browser, verifiedActors[0].name);
  await verified.context.close();
});
