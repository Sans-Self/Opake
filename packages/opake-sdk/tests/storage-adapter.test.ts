import { describe, it, expect, vi } from "vitest";
import { createStorageAdapter } from "../src/storage-adapter";
import type { WasmStorageAdapter } from "../src/storage-adapter";
import { MemoryStorage } from "../src/storage/memory";

describe("createStorageAdapter", () => {
  const EXPECTED_METHODS: readonly (keyof WasmStorageAdapter)[] = [
    "loadConfig",
    "saveConfig",
    "loadIdentity",
    "saveIdentity",
    "loadSession",
    "saveSession",
    "removeAccount",
    "cacheGetRecord",
    "cachePutRecords",
    "cacheRemoveRecord",
    "cacheGetCollection",
    "cachePutCollection",
    "cacheInvalidateCollection",
    "cacheClear",
  ] as const;

  it("has all 14 expected methods", () => {
    const adapter = createStorageAdapter(new MemoryStorage());
    for (const method of EXPECTED_METHODS) {
      expect(typeof adapter[method]).toBe("function");
    }
    expect(EXPECTED_METHODS).toHaveLength(14);
  });

  it("delegates loadConfig to the underlying Storage", async () => {
    const storage = new MemoryStorage();
    const adapter = createStorageAdapter(storage);

    const config = await adapter.loadConfig();
    expect(config).toEqual({ accounts: {} });
  });

  it("delegates saveConfig then loadConfig round-trip", async () => {
    const storage = new MemoryStorage();
    const adapter = createStorageAdapter(storage);

    const config = { accounts: { "did:plc:test": { pds_url: "https://pds.example.com", handle: "alice.test" } } };
    await adapter.saveConfig(config);
    expect(await adapter.loadConfig()).toEqual(config);
  });

  it("delegates identity methods to the underlying Storage", async () => {
    const storage = new MemoryStorage();
    const adapter = createStorageAdapter(storage);

    const identity = { did: "did:plc:x", public_key: "pub", private_key: "priv" };
    await adapter.saveIdentity("did:plc:x", identity);
    expect(await adapter.loadIdentity("did:plc:x")).toEqual(identity);
  });

  it("delegates session methods to the underlying Storage", async () => {
    const storage = new MemoryStorage();
    const adapter = createStorageAdapter(storage);

    const session = {
      type: "legacy" as const,
      did: "did:plc:x",
      handle: "alice.test",
      access_jwt: "a",
      refresh_jwt: "r",
    };
    await adapter.saveSession("did:plc:x", session);
    expect(await adapter.loadSession("did:plc:x")).toEqual(session);
  });

  it("delegates cache methods to the underlying Storage", async () => {
    const storage = new MemoryStorage();
    const adapter = createStorageAdapter(storage);
    const did = "did:plc:x";
    const collection = "app.opake.document";

    const record = { uri: "at://did:plc:x/app.opake.document/abc", cid: "cid1", value: { test: true } };
    await adapter.cachePutRecords(did, collection, [record]);

    const cached = await adapter.cacheGetRecord(did, collection, record.uri);
    expect(cached).toEqual(record);
  });

  it("calls the real Storage methods (spy verification)", async () => {
    const storage = new MemoryStorage();
    const spy = vi.spyOn(storage, "loadConfig");
    const adapter = createStorageAdapter(storage);

    await adapter.loadConfig();
    expect(spy).toHaveBeenCalledOnce();
  });
});
