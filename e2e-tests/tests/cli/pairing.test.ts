// Device pairing: pair request + pair approve
//
// SKIPPED: Requires concurrent subprocess management — Device B polls
// indefinitely while Device A approves with fingerprint confirmation.
// Needs a concurrent test helper that runs two opake processes in parallel.

import { describe, it } from "vitest";

describe.skip("device pairing (needs concurrent subprocess support)", () => {
  it.todo("pair request creates ephemeral key and polls");
  it.todo("pair approve lists requests and transfers identity");
  it.todo("new device can decrypt files after pairing");
});
