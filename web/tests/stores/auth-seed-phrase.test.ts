// Tests for seed phrase store actions — verifies they use the authenticated
// API path (with token refresh) rather than raw fetch with stale tokens.

import "fake-indexeddb/auto";
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import type { OAuthSession, Identity } from "../../src/lib/storageTypes";

const TEST_DID = "did:plc:test123";
const TEST_PDS = "https://pds.test";

const testIdentity: Identity = {
  did: TEST_DID,
  public_key: "dGVzdC1wdWJsaWMta2V5",
  private_key: "dGVzdC1wcml2YXRlLWtleQ==",
  signing_key: "dGVzdC1zaWduaW5nLWtleQ==",
  verify_key: "dGVzdC12ZXJpZnkta2V5",
};

const testSession: OAuthSession = {
  type: "oauth",
  did: TEST_DID,
  handle: "test.handle",
  accessToken: "test-access-token",
  refreshToken: "test-refresh-token",
  dpopKey: { privateKeyJwk: "{}", publicKeyJwk: "{}", thumbprint: "thumb" } as never,
  tokenEndpoint: "https://auth.test/token",
  dpopNonce: "nonce-1",
  expiresAt: Math.floor(Date.now() / 1000) + 3600,
  clientId: "https://opake.app/client-metadata.json",
};

const savedIdentities: Record<string, Identity> = {};

// Mock storage singleton used by auth store and api.ts
const mockStorage = {
  loadConfig: vi.fn().mockResolvedValue({
    defaultDid: TEST_DID,
    accounts: { [TEST_DID]: { pdsUrl: TEST_PDS, handle: "test.handle" } },
  }),
  saveConfig: vi.fn().mockResolvedValue(undefined),
  loadSession: vi.fn().mockResolvedValue(testSession),
  saveSession: vi.fn().mockResolvedValue(undefined),
  loadIdentity: vi.fn().mockRejectedValue(new Error("not found")),
  saveIdentity: vi.fn().mockImplementation((_did: string, id: Identity) => {
    savedIdentities[_did] = id;
    return Promise.resolve();
  }),
  removeAccount: vi.fn().mockResolvedValue(undefined),
  loadProfile: vi.fn().mockResolvedValue(null),
  saveProfile: vi.fn().mockResolvedValue(undefined),
};

class MockIndexedDbStorage {
  loadConfig = mockStorage.loadConfig;
  saveConfig = mockStorage.saveConfig;
  loadSession = mockStorage.loadSession;
  saveSession = mockStorage.saveSession;
  loadIdentity = mockStorage.loadIdentity;
  saveIdentity = mockStorage.saveIdentity;
  removeAccount = mockStorage.removeAccount;
  loadProfile = mockStorage.loadProfile;
  saveProfile = mockStorage.saveProfile;
}

vi.mock("../../src/lib/indexeddbStorage", () => ({
  IndexedDbStorage: MockIndexedDbStorage,
}));

// Mock opake worker
vi.mock("../../src/lib/worker", () => ({
  getOpakeWorker: () => ({
    generateMnemonic: () => "abandon ".repeat(23).trim() + " art",
    validateMnemonic: () => true,
    deriveIdentityFromMnemonic: (_phrase: string, did: string) => ({
      ...testIdentity,
      did,
    }),
    generateIdentity: (did: string) => ({ ...testIdentity, did }),
    createDpopProof: () => "mock-dpop-proof",
  }),
}));

// Mock loading helper
vi.mock("../../src/stores/app", () => ({
  loading: () => () => {},
}));

// Import after mocks are set up
const { useAuthStore } = await import("../../src/stores/auth");

describe("auth store seed phrase actions (authenticated API)", () => {
  // eslint-disable-next-line functional/no-let -- test setup
  let fetchSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    useAuthStore.setState({
      session: {
        status: "active",
        did: TEST_DID,
        handle: "test.handle",
        pdsUrl: TEST_PDS,
        avatarUrl: null,
        bannerUrl: null,
      },
      identity: { status: "fresh" },
    });

    fetchSpy = vi.spyOn(globalThis, "fetch");
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("confirmSeedPhrase calls putRecord endpoint (not raw publishPublicKey)", async () => {
    fetchSpy.mockResolvedValue(
      new Response(JSON.stringify({ uri: "at://test", cid: "cid" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );

    await useAuthStore.getState().confirmSeedPhrase("abandon ".repeat(23).trim() + " art");

    const putRecordCalls = fetchSpy.mock.calls.filter(
      ([url]) => typeof url === "string" && url.includes("putRecord"),
    );
    expect(putRecordCalls.length).toBeGreaterThan(0);

    const body = putRecordCalls[0][1]?.body as string;
    const parsed = JSON.parse(body) as Record<string, unknown>;
    expect(parsed.collection).toBe("app.opake.publicKey");
    expect(parsed.rkey).toBe("self");
  });

  it("confirmSeedPhrase sets identity to ready on success", async () => {
    fetchSpy.mockResolvedValue(
      new Response(JSON.stringify({ uri: "at://test", cid: "cid" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );

    await useAuthStore.getState().confirmSeedPhrase("abandon ".repeat(23).trim() + " art");
    expect(useAuthStore.getState().identity.status).toBe("ready");
  });

  it("confirmSeedPhrase saves identity to storage", async () => {
    fetchSpy.mockResolvedValue(
      new Response(JSON.stringify({ uri: "at://test", cid: "cid" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );

    await useAuthStore.getState().confirmSeedPhrase("abandon ".repeat(23).trim() + " art");
    expect(savedIdentities[TEST_DID]).toBeDefined();
    expect(savedIdentities[TEST_DID].public_key).toBe(testIdentity.public_key);
  });

  it("confirmSeedPhrase throws on 401 and sets identity to fresh", async () => {
    fetchSpy.mockResolvedValue(new Response("Unauthorized", { status: 401 }));

    await expect(
      useAuthStore.getState().confirmSeedPhrase("abandon ".repeat(23).trim() + " art"),
    ).rejects.toThrow();

    expect(useAuthStore.getState().identity.status).toBe("fresh");
  });

  it("recoverFromSeedPhrase calls putRecord when forced", async () => {
    // getRecord (upstream key check) → different key
    fetchSpy.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          uri: "at://test",
          cid: "cid",
          value: {
            publicKey: { $bytes: "ZGlmZmVyZW50LWtleQ==" },
            algo: "x25519",
            opakeVersion: 1,
            createdAt: new Date().toISOString(),
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );
    // putRecord → success
    fetchSpy.mockResolvedValueOnce(
      new Response(JSON.stringify({ uri: "at://test", cid: "cid" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );

    const result = await useAuthStore
      .getState()
      .recoverFromSeedPhrase("abandon ".repeat(23).trim() + " art", true);

    expect(result.mismatch).toBe(false);
    expect(useAuthStore.getState().identity.status).toBe("ready");
  });

  it("recoverFromSeedPhrase detects mismatch without forcing", async () => {
    // getRecord → different key
    fetchSpy.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          uri: "at://test",
          cid: "cid",
          value: {
            publicKey: { $bytes: "ZGlmZmVyZW50LWtleQ==" },
            algo: "x25519",
            opakeVersion: 1,
            createdAt: new Date().toISOString(),
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );

    const result = await useAuthStore
      .getState()
      .recoverFromSeedPhrase("abandon ".repeat(23).trim() + " art");

    expect(result.mismatch).toBe(true);

    // Should NOT have called putRecord
    const putRecordCalls = fetchSpy.mock.calls.filter(
      ([url]) => typeof url === "string" && url.includes("putRecord"),
    );
    expect(putRecordCalls.length).toBe(0);
  });
});
