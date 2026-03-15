import "fake-indexeddb/auto";
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { IndexedDbStorage } from "../../src/lib/indexeddbStorage";
import { StorageError } from "../../src/lib/storage";
import type {
  AccountEntry,
  Config,
  Identity,
  Session,
} from "../../src/lib/storageTypes";

let storage: IndexedDbStorage;
let dbCounter = 0;

function uniqueDbName(): string {
  return `opake-test-${++dbCounter}-${Date.now()}`;
}

const testConfig: Config = {
  defaultDid: "did:plc:alice",
  accounts: {
    "did:plc:alice": { pdsUrl: "https://pds.alice", handle: "alice.test" },
  },
};

const testIdentity: Identity = {
  did: "did:plc:alice",
  public_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
  private_key: "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=",
  signing_key: "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=",
  verify_key: "AwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwM=",
};

const testSession: Session = {
  type: "legacy",
  did: "did:plc:alice",
  handle: "alice.test",
  accessJwt: "eyJ.access.token",
  refreshJwt: "eyJ.refresh.token",
};

beforeEach(() => {
  storage = new IndexedDbStorage(uniqueDbName());
});

afterEach(async () => {
  await storage.destroy();
});

// -- Config -------------------------------------------------------------------

describe("config", () => {
  it("save and load roundtrip", async () => {
    await storage.saveConfig(testConfig);
    const loaded = await storage.loadConfig();
    expect(loaded).toEqual(testConfig);
  });

  it("load without save throws StorageError", async () => {
    await expect(storage.loadConfig()).rejects.toThrow(StorageError);
  });

  it("save overwrites previous config", async () => {
    await storage.saveConfig(testConfig);
    const updated: Config = { ...testConfig, defaultDid: "did:plc:bob" };
    await storage.saveConfig(updated);
    const loaded = await storage.loadConfig();
    expect(loaded.defaultDid).toBe("did:plc:bob");
  });

  it("handles multiple accounts", async () => {
    const config: Config = {
      defaultDid: "did:plc:alice",
      accounts: {
        "did:plc:alice": { pdsUrl: "https://pds.alice", handle: "alice.test" },
        "did:plc:bob": { pdsUrl: "https://pds.bob", handle: "bob.test" },
      },
    };
    await storage.saveConfig(config);
    const loaded = await storage.loadConfig();
    expect(Object.keys(loaded.accounts)).toHaveLength(2);
    expect(loaded.accounts["did:plc:bob"]?.handle).toBe("bob.test");
  });

  it("AccountEntry fields roundtrip through config", async () => {
    const entry: AccountEntry = {
      pdsUrl: "https://pds.carol",
      handle: "carol.test",
    };
    const config: Config = {
      defaultDid: "did:plc:carol",
      accounts: { "did:plc:carol": entry },
    };
    await storage.saveConfig(config);
    const loaded = await storage.loadConfig();
    const loadedEntry = loaded.accounts["did:plc:carol"];
    expect(loadedEntry).toBeDefined();
    expect(loadedEntry?.pdsUrl).toBe("https://pds.carol");
    expect(loadedEntry?.handle).toBe("carol.test");
  });
});

// -- Identity -----------------------------------------------------------------

describe("identity", () => {
  it("save and load roundtrip", async () => {
    await storage.saveIdentity("did:plc:alice", testIdentity);
    const loaded = await storage.loadIdentity("did:plc:alice");
    expect(loaded).toEqual(testIdentity);
  });

  it("load missing identity throws StorageError", async () => {
    await expect(storage.loadIdentity("did:plc:nobody")).rejects.toThrow(
      StorageError,
    );
  });

  it("overwrite preserves latest", async () => {
    await storage.saveIdentity("did:plc:alice", testIdentity);
    const updated: Identity = { ...testIdentity, public_key: "NEWKEY=" };
    await storage.saveIdentity("did:plc:alice", updated);
    const loaded = await storage.loadIdentity("did:plc:alice");
    expect(loaded.public_key).toBe("NEWKEY=");
  });

  it("different DIDs are independent", async () => {
    const bobIdentity: Identity = { ...testIdentity, did: "did:plc:bob" };
    await storage.saveIdentity("did:plc:alice", testIdentity);
    await storage.saveIdentity("did:plc:bob", bobIdentity);
    const alice = await storage.loadIdentity("did:plc:alice");
    const bob = await storage.loadIdentity("did:plc:bob");
    expect(alice.did).toBe("did:plc:alice");
    expect(bob.did).toBe("did:plc:bob");
  });
});

// -- Session ------------------------------------------------------------------

describe("session", () => {
  it("save and load roundtrip", async () => {
    await storage.saveSession("did:plc:alice", testSession);
    const loaded = await storage.loadSession("did:plc:alice");
    expect(loaded).toEqual(testSession);
  });

  it("load missing session throws StorageError", async () => {
    await expect(storage.loadSession("did:plc:nobody")).rejects.toThrow(
      StorageError,
    );
  });

  it("overwrite preserves latest", async () => {
    await storage.saveSession("did:plc:alice", testSession);
    const refreshed: Session = { ...testSession, accessJwt: "new.jwt" };
    await storage.saveSession("did:plc:alice", refreshed);
    const loaded = await storage.loadSession("did:plc:alice");
    expect(loaded.type).toBe("legacy");
    if (loaded.type === "legacy") {
      expect(loaded.accessJwt).toBe("new.jwt");
    }
  });
});

// -- removeAccount ------------------------------------------------------------

const multiAccountConfig: Config = {
  defaultDid: "did:plc:alice",
  accounts: {
    "did:plc:alice": { pdsUrl: "https://pds.alice", handle: "alice.test" },
    "did:plc:bob": { pdsUrl: "https://pds.bob", handle: "bob.test" },
  },
};

describe("removeAccount", () => {
  it("removes identity, session, and config entry", async () => {
    await storage.saveConfig(multiAccountConfig);
    await storage.saveIdentity("did:plc:alice", testIdentity);
    await storage.saveSession("did:plc:alice", testSession);

    await storage.removeAccount("did:plc:alice");

    await expect(storage.loadIdentity("did:plc:alice")).rejects.toThrow(
      StorageError,
    );
    await expect(storage.loadSession("did:plc:alice")).rejects.toThrow(
      StorageError,
    );
    const config = await storage.loadConfig();
    expect(config.accounts["did:plc:alice"]).toBeUndefined();
  });

  it("promotes next default when removing the current default", async () => {
    await storage.saveConfig(multiAccountConfig);
    await storage.saveIdentity("did:plc:alice", testIdentity);
    await storage.saveSession("did:plc:alice", testSession);

    await storage.removeAccount("did:plc:alice");

    const config = await storage.loadConfig();
    expect(config.defaultDid).toBe("did:plc:bob");
    expect(Object.keys(config.accounts)).toHaveLength(1);
  });

  it("clears default when removing the last account", async () => {
    await storage.saveConfig(testConfig);
    await storage.saveIdentity("did:plc:alice", testIdentity);
    await storage.saveSession("did:plc:alice", testSession);

    await storage.removeAccount("did:plc:alice");

    const config = await storage.loadConfig();
    expect(config.defaultDid).toBeNull();
    expect(Object.keys(config.accounts)).toHaveLength(0);
  });

  it("does not affect other accounts", async () => {
    await storage.saveConfig(multiAccountConfig);
    const bobIdentity: Identity = { ...testIdentity, did: "did:plc:bob" };
    const bobSession: Session = { ...testSession, did: "did:plc:bob" };
    await storage.saveIdentity("did:plc:alice", testIdentity);
    await storage.saveSession("did:plc:alice", testSession);
    await storage.saveIdentity("did:plc:bob", bobIdentity);
    await storage.saveSession("did:plc:bob", bobSession);

    await storage.removeAccount("did:plc:alice");

    const bob = await storage.loadIdentity("did:plc:bob");
    expect(bob.did).toBe("did:plc:bob");
    const bobSess = await storage.loadSession("did:plc:bob");
    expect(bobSess.did).toBe("did:plc:bob");
  });
});
