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
  cliApprovingUnverified,
  cloneCliAccount,
  login,
  pollUntil,
  removeCliAccount,
  restorePublicKey,
  setCliPdsUrl,
  startCli,
  startDropApplyWritesProxy,
  startHoldApplyWritesProxy,
  stackIsUp,
  stopCli,
  unpublishPublicKey,
  uploadTextToCabinet,
} from "../../helpers/devenv.js";
import {
  applyWritesConditional,
  deleteRepositoryRecord,
  putRepositoryRecord,
  repositoryCommit,
  repositoryRecord,
  repositoryRecords,
  type RepositoryRecord,
} from "../../e2e/pds-admin.js";
import { assertNamespace } from "../../e2e/namespace.js";

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

// The real-PDS CAS proof deliberately leaves each losing transaction's
// original intent in place while asserting the outcome. Record only its own
// rkeys so teardown is repeatable without clearing a whole namespace.
const casProofPendingRkeys = new Set<string>();
const casProofGrantRkeys = new Set<string>();

const PENDING_COLLECTION = "at.opake.pendingShare";
const GRANT_COLLECTION = "at.opake.grant";
const PUBLIC_KEY_COLLECTION = "at.opake.publicKey";
const PUBLIC_KEY_RKEY = "self";
const rkeyOfPending = (text: string): string | null => {
  const m = text.match(/at\.opake\.pendingShare\/([^\s),]+)/);
  return m?.[1] ?? null;
};

const rkeyOfUri = (uri: string): string => uri.slice(uri.lastIndexOf("/") + 1);

const pendingRkeyForDocument = (text: string, document: string): string | null => {
  const line = text.split("\n").find((candidate) => candidate.includes(document));
  return line ? rkeyOfPending(line) : null;
};

const recordValue = (record: RepositoryRecord): Record<string, unknown> => {
  if (record.value === null || typeof record.value !== "object" || Array.isArray(record.value)) {
    throw new Error(`record ${record.uri} did not contain an object value`);
  }
  return record.value as Record<string, unknown>;
};

const requireIsolatedActorNamespace = (): string => {
  const ns = (process.env.E2E_ACTOR_NS ?? "").trim();
  if (ns === "") {
    throw new Error(
      "the different-bundle retry race refuses the default fixture actors; set E2E_ACTOR_NS",
    );
  }
  return assertNamespace(ns);
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
    const owner = actorOnPds("pds-a");
    await Promise.all([
      ...[...casProofPendingRkeys].map((rkey) =>
        deleteRepositoryRecord(owner, PENDING_COLLECTION, rkey).catch(() => {}),
      ),
      ...[...casProofGrantRkeys].map((rkey) =>
        deleteRepositoryRecord(owner, GRANT_COLLECTION, rkey).catch(() => {}),
      ),
    ]);
    await stopCli();
  });

  // spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
  // spec:sharing-grants § The recipient discovers shares through the indexer, not by polling PDSes
  it("queues a share to a keyless recipient, completes it once they publish, and the recipient decrypts cross-PDS", async () => {
    const filename = uniqueName("lifecycle");
    const secret = `federation pending-share payload ${filename}`;
    const docUri = await uploadTextToCabinet(OWNER, filename, secret);

    await unpublishPublicKey(RECIPIENT);
    try {
      // Share to the not-ready recipient with --queue: the CLI warns that the
      // recipient exists but has not set up Opake, then queues (never silently).
      const shared = await cli(OWNER, [
        "share",
        "new",
        docUri,
        recipientDid,
        "--queue",
        "--allow-unverified-first-publication",
      ]);
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
      // Restart the actual CLI process before the handoff. The pending record
      // is the only durable work state, so this exercises a fresh production
      // retry runner rather than an in-memory continuation.
      // Drive retry until the grant for THIS document lands in the recipient's
      // inbox via the indexer fan-out.
      await restorePublicKey(RECIPIENT);
      await stopCli();
      await startCli();
      await login(OWNER);
      await login(RECIPIENT);
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

      // A later production retry after revocation has no intent from which to
      // recreate the grant. This guards the first-use permission against a
      // restart/revocation sequence, rather than relying on an in-process
      // runner's memory of completion.
      const afterRevoke = await cli(OWNER, ["share", "retry"]);
      expect(afterRevoke.code).toBe(0);
      expect((await cli(OWNER, ["share", "pending"])).stdout).not.toContain(docUri);
      expect(
        grantUriForDoc((await cli(RECIPIENT, ["share", "inbox", "-l"])).stdout, docUri),
      ).toBeNull();
    } finally {
      await restorePublicKey(RECIPIENT).catch(() => {});
    }
  }, 240_000);

  // spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
  // task 12.5: production API proof that two runners resolving different bundles race safely
  it("reconciles two production retry runners that resolve different first-publication bundles", async () => {
    requireIsolatedActorNamespace();
    const filename = uniqueName("different-bundles");
    const secret = `different bundles ${filename}`;
    const docUri = await uploadTextToCabinet(OWNER, filename, secret);
    const owner = actorOnPds("pds-a");
    const recipient = actorOnPds("pds-c");
    const runnerA = `${OWNER}-pending-race-${Date.now()}-${seq++}`;
    // eslint-disable-next-line functional/no-let
    let originalRecipientKey: RepositoryRecord | null = null;
    // eslint-disable-next-line functional/no-let
    let recipientKeyWasReplaced = false;

    await unpublishPublicKey(RECIPIENT);
    try {
      const queued = await cli(OWNER, [
        "share",
        "new",
        docUri,
        recipientDid,
        "--queue",
        "--allow-unverified-first-publication",
      ]);
      expect(queued.code, queued.stderr).toBe(0);
      const pendingRkey = pendingRkeyForDocument(
        (await cli(OWNER, ["share", "pending"])).stdout,
        docUri,
      );
      expect(pendingRkey, "queued intent was not listed").not.toBeNull();
      await restorePublicKey(RECIPIENT);

      originalRecipientKey = await repositoryRecord(
        recipient,
        PUBLIC_KEY_COLLECTION,
        PUBLIC_KEY_RKEY,
      );
      const replacementBundle = await repositoryRecord(
        owner,
        PUBLIC_KEY_COLLECTION,
        PUBLIC_KEY_RKEY,
      );
      expect(originalRecipientKey).not.toBeNull();
      expect(replacementBundle).not.toBeNull();
      expect(recordValue(originalRecipientKey!)["x25519PublicKey"]).not.toEqual(
        recordValue(replacementBundle!)["x25519PublicKey"],
      );

      // Runner A is a separate production CLI process with its own durable
      // local data. Its PDS URL points at a test-only hold proxy; the proxy
      // sees applyWrites only after A resolved Eve's original bundle.
      await cloneCliAccount(OWNER, runnerA);
      const proxy = await startHoldApplyWritesProxy(owner.pds);
      // eslint-disable-next-line functional/no-let -- assigned after the process starts.
      let firstRetry: ReturnType<typeof cli> | null = null;
      // eslint-disable-next-line functional/no-let -- release happens after B wins, or during cleanup.
      let proxyReleased = false;
      try {
        await setCliPdsUrl(runnerA, proxy.url);
        firstRetry = cli(runnerA, ["share", "retry"]);
        await proxy.waitUntilHeld();

        // Changing Eve's PDS record now makes runner B resolve a different,
        // still schema-valid unverified bundle. B uses the ordinary owner
        // account and production retry API; no raw grant is manufactured.
        await putRepositoryRecord(
          recipient,
          PUBLIC_KEY_COLLECTION,
          PUBLIC_KEY_RKEY,
          recordValue(replacementBundle!),
        );
        recipientKeyWasReplaced = true;

        const secondRetry = await cli(OWNER, ["share", "retry"]);
        expect(secondRetry.code, secondRetry.stderr).toBe(0);
        expect(secondRetry.stdout).toMatch(/1 completed/);

        // Capture B's committed record before letting A resume. Alice can
        // decrypt this grant only because B resolved Eve's replacement record
        // containing Alice's bundle; that proves the winner did not use A's
        // earlier resolution of Eve's original bundle.
        const bGrant = await repositoryRecord(owner, GRANT_COLLECTION, pendingRkey!);
        expect(bGrant, "B did not create the designated grant").not.toBeNull();
        expect(recordValue(bGrant!)["document"]).toBe(docUri);
        expect(recordValue(bGrant!)["recipient"]).toBe(recipientDid);
        const bDownload = await cli(OWNER, ["download", "--grant", bGrant!.uri, "--stdout"]);
        expect(bDownload.code, bDownload.stderr).toBe(0);
        expect(bDownload.stdout).toContain(secret);

        // Restore Eve before releasing A. A's CAS write must now conflict;
        // reconciliation validates B's durable grant metadata and returns
        // completion instead of publishing A's different wrap.
        await putRepositoryRecord(
          recipient,
          PUBLIC_KEY_COLLECTION,
          PUBLIC_KEY_RKEY,
          recordValue(originalRecipientKey!),
        );
        recipientKeyWasReplaced = false;
        await proxy.release();
        proxyReleased = true;

        const firstResult = await firstRetry;
        firstRetry = null;
        expect(firstResult.code, firstResult.stderr).toBe(0);
        expect(firstResult.stdout).toMatch(/1 completed/);

        expect((await cli(OWNER, ["share", "pending"])).stdout).not.toContain(docUri);
        const afterA = await repositoryRecord(owner, GRANT_COLLECTION, pendingRkey!);
        expect(afterA).not.toBeNull();
        expect(afterA!.cid).toBe(bGrant!.cid);
        expect(afterA!.value).toEqual(bGrant!.value);
        const grantsForDocument = (await repositoryRecords(owner, GRANT_COLLECTION)).filter(
          (grant) => {
            const value = recordValue(grant);
            return value["document"] === docUri && value["recipient"] === recipientDid;
          },
        );
        expect(grantsForDocument).toHaveLength(1);
        expect(grantsForDocument[0]?.uri).toBe(bGrant!.uri);
      } finally {
        if (recipientKeyWasReplaced && originalRecipientKey) {
          await putRepositoryRecord(
            recipient,
            PUBLIC_KEY_COLLECTION,
            PUBLIC_KEY_RKEY,
            recordValue(originalRecipientKey),
          ).catch(() => {});
        }
        if (!proxyReleased) await proxy.release().catch(() => {});
        if (firstRetry !== null) await firstRetry.catch(() => {});
        await proxy.stop();
        await removeCliAccount(runnerA).catch(() => {});
      }
    } finally {
      await restorePublicKey(RECIPIENT).catch(() => {});
    }
  }, 240_000);

  // spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
  it("reconciles a real committed handoff after its applyWrites response is lost", async () => {
    const filename = uniqueName("lost-response");
    const docUri = await uploadTextToCabinet(OWNER, filename, `lost response ${filename}`);

    await unpublishPublicKey(RECIPIENT);
    try {
      const queued = await cli(OWNER, [
        "share",
        "new",
        docUri,
        recipientDid,
        "--queue",
        "--allow-unverified-first-publication",
      ]);
      expect(queued.code, queued.stderr).toBe(0);
      await restorePublicKey(RECIPIENT);

      // The proxy forwards the first applyWrites to the healthy PDS, then
      // destroys the client connection after the PDS response. The CLI is
      // therefore exercising its production unknown-result reconciliation,
      // not a mock transport or a pre-completed fixture.
      const proxy = await startDropApplyWritesProxy(actorOnPds("pds-a").pds);
      await setCliPdsUrl(OWNER, proxy.url);
      try {
        const retry = await cli(OWNER, ["share", "retry"]);
        expect(retry.code, retry.stderr).toBe(0);
        expect(retry.stdout).toMatch(/1 completed/);
      } finally {
        await setCliPdsUrl(OWNER, "http://pds-a:3000");
        await proxy.stop();
      }

      expect((await cli(OWNER, ["share", "pending"])).stdout).not.toContain(docUri);
      expect(
        await pollUntil(async () => {
          const inbox = await cli(RECIPIENT, ["share", "inbox", "-l"]);
          return inbox.code === 0 && grantUriForDoc(inbox.stdout, docUri) !== null;
        }),
      ).toBe(true);
    } finally {
      await restorePublicKey(RECIPIENT).catch(() => {});
    }
  }, 180_000);

  // spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
  it("discards a pending share that has aged past its TTL on the next retry", async () => {
    const filename = uniqueName("expiry");
    const docUri = await uploadTextToCabinet(OWNER, filename, `expiring ${filename}`);

    await unpublishPublicKey(RECIPIENT);
    try {
      const shared = await cli(OWNER, [
        "share",
        "new",
        docUri,
        recipientDid,
        "--queue",
        "--allow-unverified-first-publication",
      ]);
      expect(shared.code).toBe(0);

      // Grab the just-created pending record's rkey and age it past the 7-day
      // TTL in place — a real record with real encrypted metadata, only its
      // createdAt rewritten.
      const pending = await cli(OWNER, ["share", "pending"]);
      const rkey = pendingRkeyForDocument(pending.stdout, docUri);
      expect(rkey, `no pendingShare rkey in: ${pending.stdout}`).not.toBeNull();
      await backdateRecord(OWNER, PENDING_COLLECTION, rkey!, "2020-01-01T00:00:00Z");

      // The retry pass expires (deletes) the aged record and creates no grant,
      // whether or not the recipient is ready.
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
  }, 180_000);

  // spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
  // task 8.5: real PDS proof for the compare-and-swap primitive relied on by queue consumption
  it("rejects stale or colliding create-and-consume transactions without publishing a partial grant", async () => {
    const owner = actorOnPds("pds-a");

    // Obtain one schema-valid, real grant value. The PDS CAS proof below is
    // intentionally about repository atomicity, not grant cryptography: the
    // production retry path above owns construction and decryption of grants.
    const donorDoc = await uploadTextToCabinet(
      OWNER,
      uniqueName("pds-donor"),
      "real grant envelope used by the PDS CAS proof",
    );
    const donor = await cliApprovingUnverified(OWNER, ["share", "new", donorDoc, recipientDid]);
    expect(donor.code, donor.stderr).toBe(0);
    const donorUri = donor.stdout.match(/at:\/\/[^\s]+/)?.[0];
    expect(donorUri, `no grant URI in: ${donor.stdout}`).toBeTruthy();
    const donorRkey = rkeyOfUri(donorUri!);
    casProofGrantRkeys.add(donorRkey);
    const donorRecord = await repositoryRecord(owner, GRANT_COLLECTION, donorRkey);
    expect(donorRecord).not.toBeNull();
    const grantValue = recordValue(donorRecord!);

    const queue = async (
      tag: string,
    ): Promise<{ readonly rkey: string; readonly record: RepositoryRecord }> => {
      const doc = await uploadTextToCabinet(OWNER, uniqueName(tag), `CAS proof ${tag}`);
      await unpublishPublicKey(RECIPIENT);
      try {
        const queued = await cli(OWNER, [
          "share",
          "new",
          doc,
          recipientDid,
          "--queue",
          "--allow-unverified-first-publication",
        ]);
        expect(queued.code, queued.stderr).toBe(0);
      } finally {
        await restorePublicKey(RECIPIENT);
      }
      const pending = await cli(OWNER, ["share", "pending"]);
      const rkey = pendingRkeyForDocument(pending.stdout, doc);
      expect(rkey, `no pending rkey for ${doc} in: ${pending.stdout}`).not.toBeNull();
      const record = await repositoryRecord(owner, PENDING_COLLECTION, rkey!);
      expect(record).not.toBeNull();
      casProofPendingRkeys.add(rkey!);
      return { rkey: rkey!, record: record! };
    };

    // This is the exact create(designated-rkey)+delete(intent) transaction
    // used by pending completion. Both records change together on the real
    // PDS under the revision captured before the intent read. The PDS can
    // briefly report a just-superseded commit after a prior helper write, so
    // retry the *whole observation* on InvalidSwap; each attempt still reads
    // commit, intent, then applies exactly one conditional transaction.
    const success = await queue("pds-success");
    // eslint-disable-next-line functional/no-let -- PDS can reject a stale observation.
    let applied: Awaited<ReturnType<typeof applyWritesConditional>> | null = null;
    for (let attempt = 0; attempt < 3 && applied?.status !== 200; attempt += 1) {
      // eslint-disable-next-line no-await-in-loop
      const successCommit = await repositoryCommit(owner);
      // This is deliberately after the observed revision, matching the
      // production state machine's input order.
      // eslint-disable-next-line no-await-in-loop
      expect(await repositoryRecord(owner, PENDING_COLLECTION, success.rkey)).not.toBeNull();
      // eslint-disable-next-line no-await-in-loop
      applied = await applyWritesConditional(owner, successCommit, [
        {
          $type: "com.atproto.repo.applyWrites#create",
          collection: GRANT_COLLECTION,
          rkey: success.rkey,
          value: grantValue,
        },
        {
          $type: "com.atproto.repo.applyWrites#delete",
          collection: PENDING_COLLECTION,
          rkey: success.rkey,
        },
      ]);
    }
    if (applied === null) throw new Error("no conditional applyWrites attempt ran");
    expect(applied.status, JSON.stringify(applied.json)).toBe(200);
    casProofGrantRkeys.add(success.rkey);
    expect(await repositoryRecord(owner, PENDING_COLLECTION, success.rkey)).toBeNull();
    expect(await repositoryRecord(owner, GRANT_COLLECTION, success.rkey)).not.toBeNull();

    // A replacement (same rkey, changed opaque intent value) after the
    // snapshot invalidates the whole batch: its designated grant never
    // appears and the replacement itself remains available to its owner.
    const replacement = await queue("pds-replacement");
    const replacementCommit = await repositoryCommit(owner);
    await putRepositoryRecord(owner, PENDING_COLLECTION, replacement.rkey, {
      ...recordValue(replacement.record),
      createdAt: "2021-01-01T00:00:00Z",
    });
    const staleReplacement = await applyWritesConditional(owner, replacementCommit, [
      {
        $type: "com.atproto.repo.applyWrites#create",
        collection: GRANT_COLLECTION,
        rkey: replacement.rkey,
        value: grantValue,
      },
      {
        $type: "com.atproto.repo.applyWrites#delete",
        collection: PENDING_COLLECTION,
        rkey: replacement.rkey,
      },
    ]);
    expect(staleReplacement.status, JSON.stringify(staleReplacement.json)).not.toBe(200);
    expect(staleReplacement.json).toMatchObject({ error: "InvalidSwap" });
    expect(await repositoryRecord(owner, GRANT_COLLECTION, replacement.rkey)).toBeNull();
    expect(await repositoryRecord(owner, PENDING_COLLECTION, replacement.rkey)).not.toBeNull();

    // A cancellation after the observed commit similarly cannot leave a
    // grant behind when the stale runner attempts the old transaction.
    const cancelled = await queue("pds-cancel");
    const cancelledCommit = await repositoryCommit(owner);
    const cancel = await cli(OWNER, ["share", "cancel", cancelled.record.uri]);
    expect(cancel.code, cancel.stderr).toBe(0);
    const staleCancel = await applyWritesConditional(owner, cancelledCommit, [
      {
        $type: "com.atproto.repo.applyWrites#create",
        collection: GRANT_COLLECTION,
        rkey: cancelled.rkey,
        value: grantValue,
      },
      {
        $type: "com.atproto.repo.applyWrites#delete",
        collection: PENDING_COLLECTION,
        rkey: cancelled.rkey,
      },
    ]);
    expect(staleCancel.status, JSON.stringify(staleCancel.json)).not.toBe(200);
    expect(staleCancel.json).toMatchObject({ error: "InvalidSwap" });
    expect(await repositoryRecord(owner, GRANT_COLLECTION, cancelled.rkey)).toBeNull();
    expect(await repositoryRecord(owner, PENDING_COLLECTION, cancelled.rkey)).toBeNull();

    // An unrelated queue write advances the same repository revision. It is
    // enough to make a previously observed transaction conflict; no special
    // casing based on which collection changed is safe.
    const unrelated = await queue("pds-unrelated");
    const conflicted = await queue("pds-unrelated-target");
    const unrelatedCommit = await repositoryCommit(owner);
    await queue("pds-unrelated-writer");
    const staleUnrelated = await applyWritesConditional(owner, unrelatedCommit, [
      {
        $type: "com.atproto.repo.applyWrites#create",
        collection: GRANT_COLLECTION,
        rkey: conflicted.rkey,
        value: grantValue,
      },
      {
        $type: "com.atproto.repo.applyWrites#delete",
        collection: PENDING_COLLECTION,
        rkey: conflicted.rkey,
      },
    ]);
    expect(staleUnrelated.status, JSON.stringify(staleUnrelated.json)).not.toBe(200);
    expect(staleUnrelated.json).toMatchObject({ error: "InvalidSwap" });
    expect(await repositoryRecord(owner, GRANT_COLLECTION, conflicted.rkey)).toBeNull();
    expect(await repositoryRecord(owner, PENDING_COLLECTION, conflicted.rkey)).not.toBeNull();
    expect(await repositoryRecord(owner, PENDING_COLLECTION, unrelated.rkey)).not.toBeNull();

    // A pre-existing designated grant is a collision. The PDS rejects the
    // batch before consuming its pending intent, so a runner cannot convert
    // somebody else's same-rkey record into authorization to delete work.
    const collision = await queue("pds-collision");
    const collisionCommit = await repositoryCommit(owner);
    await putRepositoryRecord(owner, GRANT_COLLECTION, collision.rkey, grantValue);
    casProofGrantRkeys.add(collision.rkey);
    const staleCollision = await applyWritesConditional(owner, collisionCommit, [
      {
        $type: "com.atproto.repo.applyWrites#create",
        collection: GRANT_COLLECTION,
        rkey: collision.rkey,
        value: grantValue,
      },
      {
        $type: "com.atproto.repo.applyWrites#delete",
        collection: PENDING_COLLECTION,
        rkey: collision.rkey,
      },
    ]);
    expect(staleCollision.status, JSON.stringify(staleCollision.json)).not.toBe(200);
    expect(staleCollision.json).toMatchObject({ error: "InvalidSwap" });
    expect(await repositoryRecord(owner, PENDING_COLLECTION, collision.rkey)).not.toBeNull();

    // The second write fails because it collides with the donor grant. The
    // first create is rolled back too: no partial designated grant appears.
    const batchFailure = await queue("pds-batch-failure");
    const batchCommit = await repositoryCommit(owner);
    const failedSecondWrite = await applyWritesConditional(owner, batchCommit, [
      {
        $type: "com.atproto.repo.applyWrites#create",
        collection: GRANT_COLLECTION,
        rkey: batchFailure.rkey,
        value: grantValue,
      },
      {
        $type: "com.atproto.repo.applyWrites#create",
        collection: GRANT_COLLECTION,
        rkey: donorRkey,
        value: grantValue,
      },
    ]);
    expect(failedSecondWrite.status, JSON.stringify(failedSecondWrite.json)).not.toBe(200);
    expect(await repositoryRecord(owner, GRANT_COLLECTION, batchFailure.rkey)).toBeNull();
    expect(await repositoryRecord(owner, PENDING_COLLECTION, batchFailure.rkey)).not.toBeNull();
  }, 300_000);
});
