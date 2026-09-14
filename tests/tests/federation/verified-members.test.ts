// Native membership verification across all three dev-env PDSes. Frank's
// anchor is created by verified-fixture.setup.ts through the production holder
// and stock PDS signer; this suite only consumes that durable state over the
// CLI/PDS/indexer boundary.
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { actorNamespace, actorsFor, type Actor } from "../../e2e/namespace.js";
import {
  publishedPublicKey,
  putRepositoryRecord,
  repositoryCommit,
  repositoryRecord,
} from "../../e2e/pds-admin.js";
import { plcDidDocument } from "../../helpers/devenv.js";
import { testEnv } from "../../helpers/pds.js";
import {
  actorByName,
  actorOnPds,
  cli,
  headUri,
  login,
  memberCount,
  pollUntil,
  stackIsUp,
  startCli,
  stopCli,
  workspaceListed,
} from "../../helpers/devenv.js";

const OWNER = actorOnPds("pds-a").name; // alice
const UNVERIFIED = actorOnPds("pds-b").name; // carol
const VERIFIED = actorByName("frank").name; // pds-c

let sequence = 0;
const workspaceName = () => `verified-native-${Date.now()}-${sequence++}`;

let unverifiedDid = "";
let verifiedDid = "";

function actor(name: string): Actor {
  const found = actorsFor(actorNamespace()).find((candidate) => candidate.name === name);
  if (!found) throw new Error(`missing fixture actor ${name}`);
  return found;
}

function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("expected record object");
  }
  return value as Record<string, unknown>;
}

function corruptSignature(value: unknown): Record<string, unknown> {
  const changed = structuredClone(record(value));
  const signature = record(changed.signature);
  const encoded = signature.$bytes;
  if (typeof encoded !== "string") throw new Error("public-key record omitted signature bytes");
  const bytes = Buffer.from(encoded, "base64");
  bytes[0] = (bytes[0] ?? 0) ^ 1;
  signature.$bytes = bytes.toString("base64");
  return changed;
}

function currentMember(head: unknown, did: string): Record<string, unknown> {
  const members = record(head).members;
  if (!Array.isArray(members)) throw new Error("keyring head omitted members");
  const member = members.find((candidate) => record(candidate).did === did);
  if (!member) throw new Error(`keyring head omitted admitted member ${did}`);
  return record(member);
}

function rkey(uri: string): string {
  return uri.slice(uri.lastIndexOf("/") + 1);
}

const base58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
function decodeBase58(value: string): Uint8Array {
  const bytes = [0];
  for (const character of value) {
    const digit = base58.indexOf(character);
    if (digit < 0) throw new Error("invalid PLC multibase key");
    let carry = digit;
    for (let index = 0; index < bytes.length; index += 1) {
      carry += bytes[index]! * 58;
      bytes[index] = carry & 0xff;
      carry >>= 8;
    }
    while (carry > 0) {
      bytes.push(carry & 0xff);
      carry >>= 8;
    }
  }
  for (const character of value) {
    if (character !== "1") break;
    bytes.push(0);
  }
  return Uint8Array.from(bytes.reverse());
}

function anchoredSigningKey(document: Record<string, unknown>): Uint8Array {
  const methods = document.verificationMethod;
  if (!Array.isArray(methods)) throw new Error("PLC DID document omitted verification methods");
  const method = methods.find(
    (candidate): candidate is { id: string; publicKeyMultibase: string } =>
      !!candidate &&
      typeof candidate === "object" &&
      typeof (candidate as { id?: unknown }).id === "string" &&
      (candidate as { id: string }).id.endsWith("#opake") &&
      typeof (candidate as { publicKeyMultibase?: unknown }).publicKeyMultibase === "string",
  );
  if (!method || !method.publicKeyMultibase.startsWith("z")) {
    throw new Error("fixture PLC DID document omitted #opake");
  }
  const decoded = decodeBase58(method.publicKeyMultibase.slice(1));
  if (decoded[0] !== 0xed || decoded[1] !== 0x01 || decoded.length !== 34) {
    throw new Error("fixture #opake method was not an Ed25519 multikey");
  }
  return decoded.slice(2);
}

describe.skipIf(testEnv() !== "devenv")("verified native membership", () => {
  beforeAll(async () => {
    if (!(await stackIsUp())) {
      throw new Error("dev-env stack is not up — run `just dev-env-up` before `just e2e-federation`");
    }
    await startCli();
    await login(OWNER);
    unverifiedDid = await login(UNVERIFIED);
    verifiedDid = await login(VERIFIED);
  }, 180_000);

  afterAll(async () => {
    await stopCli();
  });

  it("uses the matched PLC anchor without approval, refuses an unapproved bundle without writing, and rejects a corrupted anchor", async () => {
    const workspace = workspaceName();
    const verifiedActor = actor(VERIFIED);
    const originalPublicKey = await publishedPublicKey(verifiedActor);
    if (!originalPublicKey) throw new Error("verified fixture has no public-key record");

    const signingKey = record(record(originalPublicKey).signingKey).$bytes;
    if (typeof signingKey !== "string") throw new Error("verified fixture has no signing key");
    expect(anchoredSigningKey(await plcDidDocument(VERIFIED))).toEqual(
      Uint8Array.from(Buffer.from(signingKey, "base64")),
    );

    try {
      const created = await cli(OWNER, ["workspace", "create", workspace]);
      expect(created.code, created.stderr).toBe(0);
      expect(await pollUntil(() => workspaceListed(OWNER, workspace))).toBe(true);

      // A valid cross-PDS anchored member has no approval flag or prompt.
      const verifiedAdd = await cli(OWNER, ["workspace", "add-member", workspace, verifiedDid]);
      expect(verifiedAdd.code, verifiedAdd.stderr).toBe(0);
      expect(verifiedAdd.stdout + verifiedAdd.stderr).not.toContain("approve-unverified");
      expect(await pollUntil(async () => (await memberCount(OWNER, workspace)) === 2)).toBe(true);

      // The same command for an unverified cross-PDS account must reject before
      // `applyWrites`: both the owner repository revision and indexed head stay
      // exactly where the verified admission left them.
      const beforeCommit = await repositoryCommit(actor(OWNER));
      const beforeHead = await headUri(OWNER, workspace);
      const unapproved = await cli(OWNER, ["workspace", "add-member", workspace, unverifiedDid]);
      expect(unapproved.code).not.toBe(0);
      expect(unapproved.stdout + unapproved.stderr).toContain("unverified encryption key");
      expect(await repositoryCommit(actor(OWNER))).toBe(beforeCommit);
      expect(await headUri(OWNER, workspace)).toBe(beforeHead);
      expect(await memberCount(OWNER, workspace)).toBe(2);

      // The explicit flag is the third outcome: the exact current unverified
      // bundle can be admitted only after the manager acknowledges it.
      const approved = await cli(OWNER, [
        "workspace",
        "add-member",
        workspace,
        unverifiedDid,
        "--approve-unverified",
      ]);
      expect(approved.code, approved.stderr).toBe(0);
      expect(await pollUntil(async () => (await memberCount(OWNER, workspace)) === 3)).toBe(true);

      // A DID method still exists, so corrupting only the signed PDS record is
      // a verification error, never an unverified downgrade or override path.
      await putRepositoryRecord(
        verifiedActor,
        "at.opake.publicKey",
        "self",
        corruptSignature(originalPublicKey),
      );
      const removal = await cli(OWNER, ["workspace", "remove-member", workspace, unverifiedDid, "-y"]);
      expect(removal.code, removal.stderr).toBe(0);
      expect(removal.stderr).toContain(
        `${verifiedDid} remains admitted but was excluded because verification failed`,
      );
      expect(await pollUntil(async () => (await memberCount(OWNER, workspace)) === 2)).toBe(true);

      const removedHead = await headUri(OWNER, workspace);
      if (!removedHead) throw new Error("removal did not leave an indexed keyring head");
      const removedRecord = await repositoryRecord(actor(OWNER), "at.opake.keyring", rkey(removedHead));
      if (!removedRecord) throw new Error("PDS omitted the removal keyring head");
      const retainedFrank = currentMember(removedRecord.value, verifiedDid);
      expect(retainedFrank.wrappedKey).toBeUndefined();
      const history = record(removedRecord.value).keyHistory;
      expect(Array.isArray(history)).toBe(true);
      expect(
        (history as unknown[]).some((entry) => {
          const members = record(entry).members;
          return Array.isArray(members) && members.some((member) => record(member).did === verifiedDid);
        }),
      ).toBe(true);

      // Resolving a removal target by DID must not fetch its now-corrupt public
      // key record. A member can always withdraw even when its verification
      // evidence is unavailable to the manager.
      const removeCorruptFrank = await cli(OWNER, [
        "workspace",
        "remove-member",
        workspace,
        verifiedDid,
        "-y",
      ]);
      expect(removeCorruptFrank.code, removeCorruptFrank.stderr).toBe(0);
      expect(await pollUntil(async () => (await memberCount(OWNER, workspace)) === 1)).toBe(true);
    } finally {
      // Restore before Frank's next client authentication so later suites never
      // inherit an intentionally invalid signed record.
      await putRepositoryRecord(verifiedActor, "at.opake.publicKey", "self", originalPublicKey);
      const restoredLogin = await login(VERIFIED);
      expect(restoredLogin).toBe(verifiedDid);
    }
  }, 240_000);
});
