import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { describe, it, expect, beforeAll } from "vitest";
import {
  initSync,
  generateMnemonic,
  validateMnemonic,
  deriveIdentityFromMnemonic,
} from "@/wasm/opake-wasm/opake";

describe("wasm mnemonic exports", () => {
  beforeAll(async () => {
    const wasmPath = resolve(__dirname, "../../src/wasm/opake-wasm/opake_bg.wasm");
    const wasmBytes = await readFile(wasmPath);
    initSync({ module: wasmBytes });
  });

  it("generateMnemonic returns 24 space-separated words", () => {
    const phrase = generateMnemonic();
    const words = phrase.split(" ");
    expect(words).toHaveLength(24);
    words.forEach((word) => {
      expect(word).toMatch(/^[a-z]+$/);
    });
  });

  it("validateMnemonic accepts a generated mnemonic", () => {
    const phrase = generateMnemonic();
    expect(validateMnemonic(phrase)).toBe(true);
  });

  it("validateMnemonic rejects invalid input", () => {
    expect(validateMnemonic("not a valid mnemonic")).toBe(false);
    expect(validateMnemonic("")).toBe(false);
    expect(validateMnemonic("abandon ".repeat(24).trim())).toBe(false); // bad checksum
  });

  it("deriveIdentityFromMnemonic returns identity with expected fields", () => {
    const phrase = generateMnemonic();
    const identity = deriveIdentityFromMnemonic(phrase, "did:plc:test") as Record<string, unknown>;
    expect(identity.did).toBe("did:plc:test");
    expect(typeof identity.public_key).toBe("string");
    expect(typeof identity.private_key).toBe("string");
    expect(typeof identity.signing_key).toBe("string");
    expect(typeof identity.verify_key).toBe("string");
  });

  it("deriveIdentityFromMnemonic is deterministic", () => {
    const phrase = generateMnemonic();
    const id1 = deriveIdentityFromMnemonic(phrase, "did:plc:test") as Record<string, unknown>;
    const id2 = deriveIdentityFromMnemonic(phrase, "did:plc:test") as Record<string, unknown>;
    expect(id1.public_key).toBe(id2.public_key);
    expect(id1.private_key).toBe(id2.private_key);
    expect(id1.signing_key).toBe(id2.signing_key);
    expect(id1.verify_key).toBe(id2.verify_key);
  });

  it("deriveIdentityFromMnemonic throws on invalid mnemonic", () => {
    expect(() => deriveIdentityFromMnemonic("bad phrase", "did:plc:test")).toThrow();
  });

  it("different mnemonics produce different keys", () => {
    const phrase1 = generateMnemonic();
    const phrase2 = generateMnemonic();
    const id1 = deriveIdentityFromMnemonic(phrase1, "did:plc:test") as Record<string, unknown>;
    const id2 = deriveIdentityFromMnemonic(phrase2, "did:plc:test") as Record<string, unknown>;
    expect(id1.public_key).not.toBe(id2.public_key);
  });
});
