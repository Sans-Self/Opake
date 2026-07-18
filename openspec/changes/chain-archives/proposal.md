# Chain Archives

## Why

Keyring chain verification is an online walk over every historical author's PDS. Once any PDS that ever hosted a chain link goes dark, a cold-start verifier — a new member, a fresh account — can no longer reach genesis, and the workspace is permanently closed to newcomers. Existing members keep working; onboarding is what dies. For long-lived workspaces with membership churn this is a certainty on a long enough horizon, and the target deployments (organizations with member turnover) age into it fastest. Availability of history is the gap; integrity is already pinned link-by-link (`supersedesCid`), but the pins compare a *reported* CID, so they cannot authenticate bytes obtained from anywhere other than the original live host (#64).

## What Changes

- Keyring records gain an optional `chainArchive` field: an array of blob refs bundling the raw canonical-CBOR blocks of every predecessor link, genesis through head−1. Segmented because archive size scales with members × supersedes and must survive the 50 MB blob cap.
- Supersede authors maintain the archive incrementally: fetch the prior head's archive, append the prior head's own bytes, upload. O(1) work per supersede; no historical host is ever consulted.
- Cold-start verification becomes two fetches from one live host (the head author's PDS: head record + archive), then an offline walk: recompute each block's CID from bytes, match successors' `supersedesCid` pins down to genesis, confirm genesis is the workspace identity, run the existing authority walk over the parsed records.
- Clients recompute CIDs from fetched bytes (canonical dag-cbor, sha2-256 multihash, CIDv1) and compare against pins — closing #64's reported-CID gap for live walks and archives alike. The head's own CID is recomputed and checked against the indexer-reported head, so a lie at the head requires the serving PDS and the indexer to collude.
- Fallback is read-lenient: archive absent (pre-upgrade chains), malformed, or pin-mismatched degrades to today's live walk. Chains upgrade the first time any member supersedes; no flag day.
- Scope: keyring chains only. Directory chains share the failure mode with a milder symptom (degraded tree reads, not closed membership) and are explicitly out of scope.

## Capabilities

### New Capabilities

- `chain-archives`: the archive field and its maintenance contract (rolling append, segmentation), the two-fetch cold verification path, the trust argument (untrusted bytes authenticated by the pin chain; URI↔bytes binding asserted by each successor's `supersedes` + `supersedesCid` pair), and read-lenient fallback.

### Modified Capabilities

- `lineage`: the content-pin requirement's verification scope strengthens from comparing a host-reported CID to comparing a CID recomputed from fetched bytes. The v1 carve-out scenario ("a hostile host reporting a matching CID is not caught at v1") inverts: tampered bytes under a matching reported CID are rejected.

## Impact

- **Lexicon**: `at.opake.keyring` gains `chainArchive` (array of blob refs). Wire-format addition, optional, backward-readable.
- **opake-core**: `directories/chain.rs` (byte-recomputed pin comparison, archive walk), keyring supersede paths in `opake.rs` / `keyrings/` (archive maintenance on write), new dag-cbor/CID dependency (WASM-clean, no I/O).
- **Indexer**: none required — the archive is client-maintained and client-verified. The indexer's write-time authority enforcement is unchanged.
- **Issues**: implements #68, closes #64's gap; narrows #19's chain half by construction. Sibling client-side work (verified-frontier cache, #69) is out of scope here.
- **Tests**: `walk_back_does_not_catch_tampered_bytes_under_a_matching_reported_cid` flips to assert rejection.
