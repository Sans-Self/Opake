import { describe, it, expect } from "vitest";
import { MemoryStorage } from "../src/storage/memory";
import { StorageError } from "../src/storage";
import type { Identity, Session } from "../src/storage";

const TEST_DID = "did:plc:test123";

const testIdentity: Identity = {
  did: TEST_DID,
  public_key: "pub-key-base64",
  private_key: "priv-key-base64",
};

const testSession: Session = {
  type: "legacy",
  did: TEST_DID,
  handle: "alice.test",
  access_jwt: "access-token",
  refresh_jwt: "refresh-token",
};

describe("MemoryStorage", () => {
  // -- Config ---------------------------------------------------------------

  describe("config", () => {
    it("returns a default config initially", async () => {
      const storage = new MemoryStorage();
      const config = await storage.loadConfig();
      expect(config).toEqual({ accounts: {} });
    });

    it("round-trips a saved config", async () => {
      const storage = new MemoryStorage();
      const config = {
        accounts: { [TEST_DID]: { pds_url: "https://pds.example.com", handle: "alice.test" } },
        default_did: TEST_DID,
      };
      await storage.saveConfig(config);
      expect(await storage.loadConfig()).toEqual(config);
    });
  });

  // -- Identity -------------------------------------------------------------

  describe("identity", () => {
    it("throws StorageError when identity doesn't exist", async () => {
      const storage = new MemoryStorage();
      await expect(storage.loadIdentity(TEST_DID)).rejects.toThrow(StorageError);
    });

    it("round-trips a saved identity", async () => {
      const storage = new MemoryStorage();
      await storage.saveIdentity(TEST_DID, testIdentity);
      expect(await storage.loadIdentity(TEST_DID)).toEqual(testIdentity);
    });
  });

  // -- Session --------------------------------------------------------------

  describe("session", () => {
    it("throws StorageError when session doesn't exist", async () => {
      const storage = new MemoryStorage();
      await expect(storage.loadSession(TEST_DID)).rejects.toThrow(StorageError);
    });

    it("round-trips a saved session", async () => {
      const storage = new MemoryStorage();
      await storage.saveSession(TEST_DID, testSession);
      expect(await storage.loadSession(TEST_DID)).toEqual(testSession);
    });
  });

  // -- clearSession ---------------------------------------------------------

  describe("clearSession", () => {
    it("removes the session but preserves the identity", async () => {
      const storage = new MemoryStorage();
      await storage.saveIdentity(TEST_DID, testIdentity);
      await storage.saveSession(TEST_DID, testSession);

      await storage.clearSession(TEST_DID);

      // Session gone
      await expect(storage.loadSession(TEST_DID)).rejects.toThrow(StorageError);
      // Identity still there
      expect(await storage.loadIdentity(TEST_DID)).toEqual(testIdentity);
    });
  });

  // -- removeAccount --------------------------------------------------------

  describe("removeAccount", () => {
    it("removes identity, session, and cache for the account", async () => {
      const storage = new MemoryStorage();
      await storage.saveIdentity(TEST_DID, testIdentity);
      await storage.saveSession(TEST_DID, testSession);
      await storage.cachePutRecords(TEST_DID, "app.opake.document", [
        { uri: "at://test/doc/1", cid: "cid1", value: {} },
      ]);

      await storage.removeAccount(TEST_DID);

      await expect(storage.loadIdentity(TEST_DID)).rejects.toThrow(StorageError);
      await expect(storage.loadSession(TEST_DID)).rejects.toThrow(StorageError);
      expect(await storage.cacheGetRecord(TEST_DID, "app.opake.document", "at://test/doc/1")).toBeNull();
    });

    it("removes the account from config.accounts", async () => {
      const storage = new MemoryStorage();
      await storage.saveConfig({
        accounts: {
          [TEST_DID]: { pds_url: "https://pds.example.com", handle: "alice.test" },
          "did:plc:other": { pds_url: "https://pds2.example.com", handle: "bob.test" },
        },
        default_did: TEST_DID,
      });

      await storage.removeAccount(TEST_DID);

      const config = await storage.loadConfig();
      expect(config.accounts).not.toHaveProperty(TEST_DID);
      expect(config.accounts).toHaveProperty("did:plc:other");
    });

    it("reassigns default_did when the removed account was the default", async () => {
      const otherDid = "did:plc:other";
      const storage = new MemoryStorage();
      await storage.saveConfig({
        accounts: {
          [TEST_DID]: { pds_url: "https://pds.example.com", handle: "alice.test" },
          [otherDid]: { pds_url: "https://pds2.example.com", handle: "bob.test" },
        },
        default_did: TEST_DID,
      });

      await storage.removeAccount(TEST_DID);

      const config = await storage.loadConfig();
      expect(config.default_did).toBe(otherDid);
    });

    it("clears default_did when the last account is removed", async () => {
      const storage = new MemoryStorage();
      await storage.saveConfig({
        accounts: {
          [TEST_DID]: { pds_url: "https://pds.example.com", handle: "alice.test" },
        },
        default_did: TEST_DID,
      });

      await storage.removeAccount(TEST_DID);

      const config = await storage.loadConfig();
      expect(config.default_did).toBeUndefined();
      expect(Object.keys(config.accounts)).toHaveLength(0);
    });
  });
});
