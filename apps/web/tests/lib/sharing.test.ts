import { describe, expect, it } from "vitest";
import { recipientVerificationLabel } from "@/lib/sharing";

describe("recipientVerificationLabel", () => {
  it("names an unverified recipient and the confirmation consequence", () => {
    expect(
      recipientVerificationLabel({
        did: "did:plc:recipient",
        verification: "unverified",
        keyReplaced: null,
      }),
    ).toBe(
      "did:plc:recipient has a unverified encryption key. Confirmation is required before sharing.",
    );
  });

  it("names a verified recipient and a changed verification method", () => {
    expect(
      recipientVerificationLabel({
        did: "did:plc:recipient",
        verification: "verified",
        keyReplaced: true,
      }),
    ).toBe(
      "did:plc:recipient has a verified encryption key. Their verification method has changed.",
    );
  });
});
