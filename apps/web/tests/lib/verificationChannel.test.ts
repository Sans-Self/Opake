import { describe, expect, it } from "vitest";
import {
  verificationCallbackFromSearch,
  verificationChannelName,
} from "../../src/lib/verificationChannel";

describe("verification callback channel", () => {
  it("uses an operation-specific same-origin channel name", () => {
    expect(verificationChannelName("b2b6c5a0")).toBe("opake-verification:b2b6c5a0");
  });

  it("forwards code, state, and issuer together", () => {
    expect(
      verificationCallbackFromSearch("?code=code&state=csrf&iss=https%3A%2F%2Fas.test"),
    ).toEqual({ code: "code", state: "csrf", issuer: "https://as.test" });
  });

  it("rejects partial callbacks before they reach the live operation", () => {
    expect(verificationCallbackFromSearch("?code=code&state=csrf")).toBeNull();
  });
});
