// §5 from AGENT-BLACKBOX-TEST.md: Sharing (Grants)
//
// SKIPPED: Sharing requires recipient identity resolution (handle → DID →
// PDS → publicKey), which needs .well-known/atproto-did and DID document
// hosting in fake-pds. Tracked as a future fake-pds enhancement.

import { describe, it } from "vitest";

describe.skip("sharing (needs DID resolution in fake-pds)", () => {
  it.todo("alice uploads and shares with bob, bob downloads via grant");
  it.todo("alice can list shared grants");
  it.todo("alice revokes grant, bob download fails");
});
