import { describe, expect, it } from "vitest";
import { mapTaskKind } from "@/stores/tasks";

describe("background task mapping", () => {
  it("retains queued-share completion notices instead of only failures", () => {
    const task = mapTaskKind({
      type: "shareRetry",
      retried: 1,
      verificationErrors: [],
      completionNotices: [{
        did: "did:plc:alice",
        verification: { state: "verified", anchorHistory: "unavailable" },
      }],
    });

    expect(task).toMatchObject({
      type: "shareRetry",
      completionNotices: [{ did: "did:plc:alice" }],
    });
  });

  it("retains repair deferrals and final verification notices", () => {
    const task = mapTaskKind({
      type: "memberWrapRepair",
      repaired: 1,
      verificationNotices: [{
        did: "did:plc:bob",
        verification: { state: "verified", anchorHistory: "replaced" },
      }],
      awaitingApproval: 2,
      verificationFailed: 3,
      deferredHumanDecision: 4,
      deferredVisibility: 5,
      deferredByBudget: 6,
      discoveryDeferred: true,
    });

    expect(task).toMatchObject({
      type: "memberWrapRepair",
      repaired: 1,
      deferredByBudget: 6,
      discoveryDeferred: true,
      verificationNotices: [{ did: "did:plc:bob" }],
    });
  });
});
