// Pending-share queue lifecycle against the real dev-env: a share to a
// recipient who has not published an Opake public key is queued (not dropped),
// completes once they publish, and the recipient then decrypts the document
// cross-PDS. A separate case ages a real queued record past its TTL and proves
// the retry pass discards it. Driven through the opake CLI and asserted through
// the indexer-backed inbox — the same pipeline a real client rides.
//
// The RecipientNotReady branch is otherwise unreachable in tests: the dev-env
// bootstrap seeds every actor with a published key. `unpublishPublicKey`
// deletes the recipient's `publicKey/self` over XRPC (test-support only, no
// product code) and `restorePublicKey` puts the stashed record back, so the
// shared fixture actor is left exactly as found.
//
// Only runs under OPAKE_TEST_ENV=devenv (`just e2e-federation`).

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { testEnv } from "../../helpers/pds.js";
import {
  actorOnPds,
  backdateRecord,
  cli,
  login,
  pollUntil,
  restorePublicKey,
  startCli,
  stackIsUp,
  stopCli,
  unpublishPublicKey,
  uploadTextToCabinet,
} from "../../helpers/devenv.js";

// Owner and recipient on DIFFERENT PDSes so every grant crosses a federation
// boundary. Foreign identities are addressed BY DID (handle→DID for foreign
// actor subdomains is the known dev-env gap); by-DID resolution is fully local.
const OWNER = actorOnPds("pds-a").name; // alice
const RECIPIENT = actorOnPds("pds-c").name; // eve

// eslint-disable-next-line functional/no-let
let seq = 0;
const uniqueName = (tag: string): string => `share-${tag}-${Date.now()}-${seq++}.txt`;

// eslint-disable-next-line functional/no-let
let recipientDid = "";

const PENDING_COLLECTION = "at.opake.pendingShare";
const rkeyOfPending = (text: string): string | null => {
  const m = text.match(/at\.opake\.pendingShare\/([^\s),]+)/);
  return m?.[1] ?? null;
};

// Fixture actors accumulate inbox grants across runs, so a test must isolate
// the grant for ITS OWN document rather than grabbing "the first grant". The
// long inbox format pairs each `doc:` with the following `grant:`; this maps
// a document URI to its grant URI, or null if the doc isn't in the inbox.
const grantUriForDoc = (inboxLongStdout: string, docUri: string): string | null => {
  const re = /doc:\s*(\S+)\s*\n\s*grant:\s*(\S+)/g;
  // eslint-disable-next-line functional/no-let
  let m: RegExpExecArray | null;
  while ((m = re.exec(inboxLongStdout)) !== null) {
    if (m[1] === docUri) return m[2] ?? null;
  }
  return null;
};

describe.skipIf(testEnv() !== "devenv")("pending-share queue", () => {
  beforeAll(async () => {
    if (!(await stackIsUp())) {
      throw new Error(
        "dev-env stack is not up — run `just dev-env-up` before `just e2e-federation`",
      );
    }
    await startCli();
    await login(OWNER);
    recipientDid = await login(RECIPIENT);
  }, 120_000);

  afterAll(async () => {
    // Belt-and-braces: leave the recipient ready even if a test threw mid-flight.
    await restorePublicKey(RECIPIENT).catch(() => {});
    await stopCli();
  });

  it(
    // spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
    // spec:sharing-grants § The recipient discovers shares through the indexer, not by polling PDSes
    "queues a share to a keyless recipient, completes it once they publish, and the recipient decrypts cross-PDS",
    async () => {
      const filename = uniqueName("lifecycle");
      const secret = `federation pending-share payload ${filename}`;
      const docUri = await uploadTextToCabinet(OWNER, filename, secret);

      await unpublishPublicKey(RECIPIENT);
      try {
        // Share to the not-ready recipient with --queue: the CLI warns that the
        // recipient exists but has not set up Opake, then queues (never silently).
        const shared = await cli(OWNER, ["share", "new", docUri, recipientDid, "--queue"]);
        expect(shared.code).toBe(0);
        expect(shared.stdout).toContain("hasn't set up Opake");
        expect(shared.stdout.toLowerCase()).toContain("queued");

        // The pending record is on the owner's PDS (not indexed) and lists the doc.
        const pending = await cli(OWNER, ["share", "pending"]);
        expect(pending.code).toBe(0);
        expect(pending.stdout).toContain(docUri);

        // A retry while the recipient is still keyless leaves this doc queued.
        // (The shared fixture actor accretes pending shares across runs, so
        // assertions target THIS document, never global queue totals.)
        const earlyRetry = await cli(OWNER, ["share", "retry"]);
        expect(earlyRetry.code).toBe(0);
        expect(earlyRetry.stdout).toMatch(/still pending/);
        expect((await cli(OWNER, ["share", "pending"])).stdout).toContain(docUri);

        // The recipient publishes their key; retrying now completes the grant.
        // Drive retry until the grant for THIS document lands in the recipient's
        // inbox via the indexer fan-out.
        await restorePublicKey(RECIPIENT);
        // eslint-disable-next-line functional/no-let
        let grantUri: string | null = null;
        expect(
          await pollUntil(async () => {
            await cli(OWNER, ["share", "retry"]);
            const inbox = await cli(RECIPIENT, ["share", "inbox", "-l"]);
            grantUri = grantUriForDoc(inbox.stdout, docUri);
            return inbox.code === 0 && grantUri !== null;
          }),
        ).toBe(true);

        // This document's pending record is drained (others may remain).
        expect((await cli(OWNER, ["share", "pending"])).stdout).not.toContain(docUri);

        // The recipient downloads from the grant and decrypts the original bytes
        // — the whole point, cross-PDS: grant on pds-a, blob on pds-a, unwrapped
        // with the recipient's own keys on pds-c.
        const dl = await cli(RECIPIENT, ["download", "--grant", grantUri!, "--stdout"]);
        expect(dl.code).toBe(0);
        expect(dl.stdout).toContain(secret);

        // Revoke stops future discovery: the grant drops from the inbox.
        const revoked = await cli(OWNER, ["share", "revoke", grantUri!, "-y"]);
        expect(revoked.code).toBe(0);
        expect(
          await pollUntil(
            async () => {
              const inbox = await cli(RECIPIENT, ["share", "inbox", "-l"]);
              return inbox.code === 0 && grantUriForDoc(inbox.stdout, docUri) === null;
            },
            { timeoutMs: 90_000, intervalMs: 2_000 },
          ),
        ).toBe(true);
      } finally {
        await restorePublicKey(RECIPIENT).catch(() => {});
      }
    },
    240_000,
  );

  it(
    // spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
    "discards a pending share that has aged past its TTL on the next retry",
    async () => {
      const filename = uniqueName("expiry");
      const docUri = await uploadTextToCabinet(OWNER, filename, `expiring ${filename}`);

      await unpublishPublicKey(RECIPIENT);
      try {
        const shared = await cli(OWNER, ["share", "new", docUri, recipientDid, "--queue"]);
        expect(shared.code).toBe(0);

        // Grab the just-created pending record's rkey and age it past the 7-day
        // TTL in place — a real record with real encrypted metadata, only its
        // createdAt rewritten.
        const pending = await cli(OWNER, ["share", "pending"]);
        const rkey = rkeyOfPending(pending.stdout);
        expect(rkey, `no pendingShare rkey in: ${pending.stdout}`).not.toBeNull();
        await backdateRecord(OWNER, PENDING_COLLECTION, rkey!, "2020-01-01T00:00:00Z");

        // The retry pass expires (deletes) the aged record and creates no grant,
        // whether or not the recipient is ready — expiry is checked first.
        const retry = await cli(OWNER, ["share", "retry"]);
        expect(retry.code).toBe(0);
        expect(retry.stdout).toMatch(/1 expired/);

        const after = await cli(OWNER, ["share", "pending"]);
        expect(after.stdout).not.toContain(docUri);

        // No grant was minted for the expired share: the recipient's inbox
        // (long form, which lists document URIs) carries no grant for this doc.
        await restorePublicKey(RECIPIENT);
        const inbox = await cli(RECIPIENT, ["share", "inbox", "-l"]);
        expect(grantUriForDoc(inbox.stdout, docUri)).toBeNull();
      } finally {
        await restorePublicKey(RECIPIENT).catch(() => {});
      }
    },
    180_000,
  );
});
