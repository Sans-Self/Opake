import { describe, expect, it } from "vitest";
import {
  counterpartyVerificationBadge,
  recipientVerificationLabel,
  recipientWriteVerificationNotice,
  writeVerificationSeverity,
} from "@/lib/sharing";

describe("recipientVerificationLabel", () => {
  it("names an unverified recipient and the confirmation consequence", () => {
    expect(
      recipientVerificationLabel({
        did: "did:plc:recipient",
        verification: "unverified",
        anchorHistory: null,
      }),
    ).toBe(
      "did:plc:recipient's encryption key is unverified. Confirmation is required before sharing.",
    );
  });

  it("names a verified recipient and a changed verification method", () => {
    expect(
      recipientVerificationLabel({
        did: "did:plc:recipient",
        verification: "verified",
        anchorHistory: "replaced",
      }),
    ).toBe(
      "did:plc:recipient's encryption key is verified. Their verification method has changed.",
    );
  });
});

describe("recipientWriteVerificationNotice", () => {
  it("distinguishes a replaced method, an absent history and an unreadable one", () => {
    expect(
      recipientWriteVerificationNotice({ state: "verified", anchorHistory: "replaced" }),
    ).toBe("Their DID verification method has changed.");
    expect(
      recipientWriteVerificationNotice({ state: "verified", anchorHistory: "noHistory" }),
    ).toBe("Their DID method publishes no verification history to read.");
    expect(
      recipientWriteVerificationNotice({ state: "verified", anchorHistory: "unavailable" }),
    ).toBe("Their verification history could not be read, so a replacement cannot be ruled out.");
    expect(
      recipientWriteVerificationNotice({ state: "verified", anchorHistory: "notReplaced" }),
    ).toBeNull();
  });

  it("marks a replaced or unreadable history as a warning, never a success", () => {
    expect(writeVerificationSeverity({ state: "verified", anchorHistory: "replaced" })).toBe("warning");
    expect(writeVerificationSeverity({ state: "verified", anchorHistory: "unavailable" })).toBe("warning");
    expect(writeVerificationSeverity({ state: "verified", anchorHistory: "noHistory" })).toBe("success");
    expect(writeVerificationSeverity({ state: "unverified" })).toBe("success");
  });
});

describe("counterpartyVerificationBadge", () => {
  it("distinguishes every resolution outcome in a short label", () => {
    expect(counterpartyVerificationBadge({ verification: "unverified", anchorHistory: null })).toBe("unverified");
    expect(counterpartyVerificationBadge({ verification: "verified", anchorHistory: "notReplaced" })).toBe("verified");
    expect(counterpartyVerificationBadge({ verification: "verified", anchorHistory: "replaced" })).toBe("verified, method changed");
    expect(counterpartyVerificationBadge({ verification: "verified", anchorHistory: "noHistory" })).toBe("verified, no history");
    expect(counterpartyVerificationBadge({ verification: "verified", anchorHistory: "unavailable" })).toBe("verified, history unavailable");
  });
});
