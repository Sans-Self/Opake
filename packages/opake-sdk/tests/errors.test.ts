import { describe, it, expect } from "vitest";
import { parseWasmError, OpakeError } from "../src/errors";

describe("parseWasmError", () => {
  it("parses a known kind prefix into a typed OpakeError", () => {
    const err = parseWasmError("Auth: token expired");
    expect(err).toBeInstanceOf(OpakeError);
    expect(err.kind).toBe("Auth");
    expect(err.message).toBe("token expired");
  });

  it("falls back to Unknown for an unrecognized prefix", () => {
    const err = parseWasmError("WeirdThing: foo");
    expect(err.kind).toBe("Unknown");
    expect(err.message).toBe("WeirdThing: foo");
  });

  it("falls back to Unknown when there is no colon", () => {
    const err = parseWasmError("something broke");
    expect(err.kind).toBe("Unknown");
    expect(err.message).toBe("something broke");
  });

  it("extracts the message from an Error object", () => {
    const err = parseWasmError(new Error("NotFound: record missing"));
    expect(err.kind).toBe("NotFound");
    expect(err.message).toBe("record missing");
  });

  it("handles an empty string", () => {
    const err = parseWasmError("");
    expect(err.kind).toBe("Unknown");
    expect(err.message).toBe("");
  });

  it("handles every known kind", () => {
    const kinds = [
      "NotFound",
      "Auth",
      "Encryption",
      "Decryption",
      "KeyWrap",
      "InvalidRecord",
      "Storage",
      "Xrpc",
      "Appview",
      "AlreadyExists",
      "AmbiguousName",
      "Serialization",
      "Mnemonic",
    ] as const;

    for (const kind of kinds) {
      const err = parseWasmError(`${kind}: detail`);
      expect(err.kind).toBe(kind);
      expect(err.message).toBe("detail");
    }
  });

  it("preserves colons in the message portion", () => {
    const err = parseWasmError("Auth: url: https://example.com");
    expect(err.kind).toBe("Auth");
    expect(err.message).toBe("url: https://example.com");
  });
});
