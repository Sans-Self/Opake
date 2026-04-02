import "fake-indexeddb/auto";
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { IndexedDbStorage } from "../../src/lib/indexeddbStorage";
import type { CachedRecord, CachedCollection } from "../../src/lib/storageTypes";

let storage: IndexedDbStorage;
let dbCounter = 0;

function uniqueDbName(): string {
  return `opake-cache-test-${++dbCounter}-${Date.now()}`;
}

const DID = "did:plc:alice";
const COLLECTION = "app.opake.document";

function fakeRecord(rkey: string, name: string): CachedRecord<{ name: string }> {
  return {
    uri: `at://${DID}/${COLLECTION}/${rkey}`,
    cid: `bafycid${rkey}`,
    value: { name },
  };
}

beforeEach(() => {
  storage = new IndexedDbStorage(uniqueDbName());
});

afterEach(async () => {
  await storage.destroy();
});

// -- Record-level -------------------------------------------------------------

describe("cacheGetRecord / cachePutRecords", () => {
  it("returns null for uncached record", async () => {
    const result = await storage.cacheGetRecord(DID, COLLECTION, "at://nope");
    expect(result).toBeNull();
  });

  it("put then get roundtrips a single record", async () => {
    const record = fakeRecord("r1", "alpha");
    await storage.cachePutRecords(DID, COLLECTION, [record]);
    const loaded = await storage.cacheGetRecord(DID, COLLECTION, record.uri);
    expect(loaded).toEqual(record);
  });

  it("put multiple records and get each individually", async () => {
    const records = [fakeRecord("r1", "alpha"), fakeRecord("r2", "beta")];
    await storage.cachePutRecords(DID, COLLECTION, records);

    const r1 = await storage.cacheGetRecord(DID, COLLECTION, records[0].uri);
    const r2 = await storage.cacheGetRecord(DID, COLLECTION, records[1].uri);
    expect(r1?.value).toEqual({ name: "alpha" });
    expect(r2?.value).toEqual({ name: "beta" });
  });

  it("upserts existing records (same URI, new CID)", async () => {
    const original = fakeRecord("r1", "original");
    await storage.cachePutRecords(DID, COLLECTION, [original]);

    const updated: CachedRecord<{ name: string }> = {
      ...original,
      cid: "bafynewcid",
      value: { name: "updated" },
    };
    await storage.cachePutRecords(DID, COLLECTION, [updated]);

    const loaded = await storage.cacheGetRecord(DID, COLLECTION, original.uri);
    expect(loaded?.cid).toBe("bafynewcid");
    expect(loaded?.value).toEqual({ name: "updated" });
  });

  it("isolates records by DID", async () => {
    const aliceRecord = fakeRecord("r1", "alice-doc");
    const bobRecord: CachedRecord<{ name: string }> = {
      ...fakeRecord("r1", "bob-doc"),
      uri: `at://did:plc:bob/${COLLECTION}/r1`,
    };

    await storage.cachePutRecords(DID, COLLECTION, [aliceRecord]);
    await storage.cachePutRecords("did:plc:bob", COLLECTION, [bobRecord]);

    const alice = await storage.cacheGetRecord(DID, COLLECTION, aliceRecord.uri);
    const bob = await storage.cacheGetRecord("did:plc:bob", COLLECTION, bobRecord.uri);
    expect(alice?.value).toEqual({ name: "alice-doc" });
    expect(bob?.value).toEqual({ name: "bob-doc" });
  });

  it("isolates records by collection", async () => {
    const docRecord = fakeRecord("r1", "a-doc");
    const dirRecord: CachedRecord<{ name: string }> = {
      uri: `at://${DID}/app.opake.directory/r1`,
      cid: "bafydir",
      value: { name: "a-dir" },
    };

    await storage.cachePutRecords(DID, COLLECTION, [docRecord]);
    await storage.cachePutRecords(DID, "app.opake.directory", [dirRecord]);

    const doc = await storage.cacheGetRecord(DID, COLLECTION, docRecord.uri);
    const dir = await storage.cacheGetRecord(DID, "app.opake.directory", dirRecord.uri);
    expect(doc?.value).toEqual({ name: "a-doc" });
    expect(dir?.value).toEqual({ name: "a-dir" });
  });
});

describe("cacheRemoveRecord", () => {
  it("removes a specific record", async () => {
    const records = [fakeRecord("r1", "alpha"), fakeRecord("r2", "beta")];
    await storage.cachePutRecords(DID, COLLECTION, records);

    await storage.cacheRemoveRecord(DID, COLLECTION, records[0].uri);

    const removed = await storage.cacheGetRecord(DID, COLLECTION, records[0].uri);
    const kept = await storage.cacheGetRecord(DID, COLLECTION, records[1].uri);
    expect(removed).toBeNull();
    expect(kept).not.toBeNull();
  });

  it("no-op for nonexistent record", async () => {
    await storage.cacheRemoveRecord(DID, COLLECTION, "at://nope");
  });
});

// -- Collection-level ---------------------------------------------------------

describe("cacheGetCollection / cachePutCollection", () => {
  it("returns null when collection was never fully fetched", async () => {
    const result = await storage.cacheGetCollection(DID, COLLECTION);
    expect(result).toBeNull();
  });

  it("returns null even if individual records exist (no fetchedAt)", async () => {
    await storage.cachePutRecords(DID, COLLECTION, [fakeRecord("r1", "alpha")]);
    const result = await storage.cacheGetCollection(DID, COLLECTION);
    expect(result).toBeNull();
  });

  it("put collection then get roundtrips", async () => {
    const data: CachedCollection<{ name: string }> = {
      records: [fakeRecord("r1", "alpha"), fakeRecord("r2", "beta")],
      fetchedAt: 1700000000000,
    };
    await storage.cachePutCollection(DID, COLLECTION, data);

    const loaded = await storage.cacheGetCollection<{ name: string }>(DID, COLLECTION);
    expect(loaded).not.toBeNull();
    expect(loaded!.fetchedAt).toBe(1700000000000);
    expect(loaded!.records).toHaveLength(2);
    expect(loaded!.records.map((r) => r.value.name).sort()).toEqual(["alpha", "beta"]);
  });

  it("replaces previous collection atomically", async () => {
    const v1: CachedCollection<{ name: string }> = {
      records: [fakeRecord("r1", "old-a"), fakeRecord("r2", "old-b")],
      fetchedAt: 1000,
    };
    await storage.cachePutCollection(DID, COLLECTION, v1);

    const v2: CachedCollection<{ name: string }> = {
      records: [fakeRecord("r3", "new-c")],
      fetchedAt: 2000,
    };
    await storage.cachePutCollection(DID, COLLECTION, v2);

    const loaded = await storage.cacheGetCollection<{ name: string }>(DID, COLLECTION);
    expect(loaded!.fetchedAt).toBe(2000);
    expect(loaded!.records).toHaveLength(1);
    expect(loaded!.records[0].value.name).toBe("new-c");

    // Old records should be gone
    const oldR1 = await storage.cacheGetRecord(DID, COLLECTION, `at://${DID}/${COLLECTION}/r1`);
    expect(oldR1).toBeNull();
  });

  it("individual put records are visible in get collection after putCollection sets fetchedAt", async () => {
    await storage.cachePutCollection(DID, COLLECTION, {
      records: [fakeRecord("r1", "alpha")],
      fetchedAt: 1000,
    });

    // Add a record individually (simulating ensureDirectoryReady cache write)
    await storage.cachePutRecords(DID, COLLECTION, [fakeRecord("r2", "beta")]);

    const loaded = await storage.cacheGetCollection<{ name: string }>(DID, COLLECTION);
    expect(loaded!.records).toHaveLength(2);
  });
});

describe("cacheInvalidateCollection", () => {
  it("clears fetchedAt so getCollection returns null", async () => {
    await storage.cachePutCollection(DID, COLLECTION, {
      records: [fakeRecord("r1", "alpha")],
      fetchedAt: 1000,
    });

    await storage.cacheInvalidateCollection(DID, COLLECTION);

    const collection = await storage.cacheGetCollection(DID, COLLECTION);
    expect(collection).toBeNull();
  });

  it("preserves individual records after invalidation", async () => {
    await storage.cachePutCollection(DID, COLLECTION, {
      records: [fakeRecord("r1", "alpha")],
      fetchedAt: 1000,
    });

    await storage.cacheInvalidateCollection(DID, COLLECTION);

    const record = await storage.cacheGetRecord(DID, COLLECTION, `at://${DID}/${COLLECTION}/r1`);
    expect(record).not.toBeNull();
    expect(record!.value).toEqual({ name: "alpha" });
  });
});

// -- Account-level ------------------------------------------------------------

describe("cacheClear", () => {
  it("removes all cache data for an account", async () => {
    await storage.cachePutCollection(DID, COLLECTION, {
      records: [fakeRecord("r1", "alpha")],
      fetchedAt: 1000,
    });
    await storage.cachePutCollection(DID, "app.opake.directory", {
      records: [{ uri: `at://${DID}/app.opake.directory/d1`, cid: "bafydir", value: { name: "dir" } }],
      fetchedAt: 1000,
    });

    await storage.cacheClear(DID);

    expect(await storage.cacheGetCollection(DID, COLLECTION)).toBeNull();
    expect(await storage.cacheGetCollection(DID, "app.opake.directory")).toBeNull();
    expect(await storage.cacheGetRecord(DID, COLLECTION, `at://${DID}/${COLLECTION}/r1`)).toBeNull();
  });

  it("does not affect other accounts", async () => {
    await storage.cachePutCollection(DID, COLLECTION, {
      records: [fakeRecord("r1", "alice-doc")],
      fetchedAt: 1000,
    });

    const bobUri = `at://did:plc:bob/${COLLECTION}/r1`;
    await storage.cachePutCollection("did:plc:bob", COLLECTION, {
      records: [{ uri: bobUri, cid: "bafybob", value: { name: "bob-doc" } }],
      fetchedAt: 1000,
    });

    await storage.cacheClear(DID);

    expect(await storage.cacheGetRecord(DID, COLLECTION, `at://${DID}/${COLLECTION}/r1`)).toBeNull();
    const bobRecord = await storage.cacheGetRecord("did:plc:bob", COLLECTION, bobUri);
    expect(bobRecord).not.toBeNull();
    expect(bobRecord!.value).toEqual({ name: "bob-doc" });
  });
});

// -- removeAccount clears cache -----------------------------------------------

describe("removeAccount clears cache", () => {
  it("removes cached records alongside identity/session/config", async () => {
    const config = {
      defaultDid: DID,
      accounts: { [DID]: { pdsUrl: "https://pds.test", handle: "alice.test" } },
    };
    await storage.saveConfig(config);
    await storage.saveIdentity(DID, {
      did: DID,
      public_key: "AAAA",
      private_key: "BBBB",
      signing_key: null,
      verify_key: null,
    });
    await storage.saveSession(DID, {
      type: "legacy",
      did: DID,
      handle: "alice.test",
      accessJwt: "jwt",
      refreshJwt: "jwt",
    });
    await storage.cachePutCollection(DID, COLLECTION, {
      records: [fakeRecord("r1", "alpha")],
      fetchedAt: 1000,
    });

    await storage.removeAccount(DID);

    expect(await storage.cacheGetCollection(DID, COLLECTION)).toBeNull();
    expect(await storage.cacheGetRecord(DID, COLLECTION, `at://${DID}/${COLLECTION}/r1`)).toBeNull();
  });
});
