# Design — Chain Archives

## Context

Keyring chains are verified by an online walk (`walk_back_to_genesis`, `verify_and_walk_chain` in `directories/chain.rs`): one `getRecord` per link against whichever PDS hosts it, pin-checked against the CID the host reports. Integrity is location-bound — a link can only be authenticated by fetching it from its original live host, because the pin comparison trusts that host's reported CID (`spec:lineage § Supersede references carry a content pin`, v1 scope). When a historical author's PDS dies, the links it hosted become unverifiable for anyone without a prior verified frontier, and cold-start verification — the new-member path — is closed forever.

Two properties of the existing system make the fix tractable: chain history is immutable (a verified suffix never changes), and every link is already pinned by its successor (`supersedes` + `supersedesCid`). What is missing is (a) byte-level pin verification, so bytes can be authenticated regardless of who serves them (#64), and (b) a place where the bytes reliably live (#68).

## Goals / Non-Goals

**Goals:**

- A workspace can onboard new members with every historical PDS dead, so long as the current head's PDS is alive.
- Cold-start verification cost independent of how many hosts history is scattered across: O(1) fetches, offline walk.
- Byte-level tamper evidence for chain links, from archives and live walks alike.
- No new trust anchors: verification still bottoms out at genesis-URI-equals-workspace-identity.

**Non-Goals:**

- Directory chains. Same failure mode, milder symptom (degraded tree reads); the mechanism ports later if warranted.
- Document custody/replication (#19's remaining half). Archives carry keyring records only.
- Hot-path walk cost for established devices — that is the frontier cache (#69), client-side, separately tracked.
- Revocation, rotation, or any change to what the chain *means* — this is availability and verification strength only.

## Decisions

**D1 — Archive container: CARv1, blocks in genesis→head−1 chain order.**
Each segment is a CARv1 file of dag-cbor blocks. CAR is atproto's native block container — `goat` and the wider tooling ecosystem can inspect archives without Opake-specific code. Verification never trusts the CIDs embedded in CAR framing: every block's CID is recomputed from its bytes and matched against the successor's pin, so the framing is transport, not trust. Alternative considered: a bespoke length-prefixed block sequence — marginally simpler, loses tooling interop, saves nothing that matters.

**D2 — `chainArchive` is an array of blob refs, chain-ordered, segments split below the blob cap.**
Archive size scales with members × supersedes (a member entry carries a hybrid wrapped key; the ML-KEM-768 ciphertext alone is ~1.1 KB — a 100-member workspace has ~130 KB keyring records, and a thousand-supersede history of those is ~130 MB). Bytes can be neither elided nor compressed (CID recomputation needs exact bytes; wrapped keys are incompressible), so segmentation is a wire-format requirement, not an optimization. Segments split at a soft threshold comfortably under the 50 MB default blob cap; the array concatenates into one logical block sequence.

**D3 — Byte-binding: canonical dag-cbor + sha2-256 multihash, CIDv1, in opake-core.**
`serde_ipld_dagcbor` + `cid`/`multihash` crates — WASM-clean, no I/O, no async. The encoding must byte-match atproto's canonical form exactly; a silent canonicalization mismatch would make every honest pin comparison fail closed. Guard with known-answer tests against records fetched from a real PDS (the KAT harness direction in #52 is the natural home). This lands for live walks too: `fetch_with_cache` recomputes the fetched record's CID and the walk compares recomputed-vs-pinned, retiring the reported-CID comparison. The head's recomputed CID is checked against the indexer-reported head CID — a lie at the head then requires the serving PDS and the indexer to collude.

**D4 — Authors verify-before-extend; a bad prior archive is rebuilt, never propagated.**
A supersede author already walks and verifies the chain before writing. Archive maintenance extends this: fetch the prior head's archive, verify it against the just-walked chain (recomputed CIDs, pin matches), append the prior head's bytes, upload. If the prior archive is missing, malformed, or disagrees with the verified chain, the author rebuilds from its own verified walk instead of extending — archives self-heal and poison never propagates. The append itself is O(1); a rebuild costs one full walk, which the author was doing anyway.

**D5 — Fallback is read-lenient, mirroring existing chain dispositions.**
Cold verification order: archive path first (two fetches, offline walk); on any archive defect, degrade to the live walk (today's behavior); on live-walk failure, degrade to the newest fully-verifiable state, exactly as pin mismatches do now (`spec:tree-chains § Consumers build the live tree from chain heads only` disposition). An archive can make verification succeed where the live walk cannot; it can never make verification accept what the live walk would reject.

**D6 — The trust argument, stated once.**
The archive is untrusted storage authenticated by the pin chain. Induction: the head is fetched live and its recomputed CID checked against the indexer's report; the head's pin authenticates the predecessor's bytes; each link's pin authenticates the next; the first supersede's pin authenticates the genesis bytes; the genesis URI must equal the workspace identity. Record bytes do not contain their own URI, so the URI↔bytes binding for every link is asserted by its successor's `supersedes` + `supersedesCid` pair — this pairing is load-bearing and the spec says so explicitly. Nobody trusts the archiver; they can only fail to produce bytes that hash correctly.

**D7 — The indexer neither builds, serves, nor validates archives.**
Client-maintained, client-verified, hosted on the head author's PDS as ordinary blobs. Keeping the indexer out preserves its discovery-only posture and avoids it becoming load-bearing for verification.

## Risks / Trade-offs

- [Canonical-form mismatch between client dag-cbor and atproto's] → KATs against real PDS records before the comparison is enforced; the existing limitation test (`walk_back_does_not_catch_tampered_bytes_under_a_matching_reported_cid`) flips to assert rejection and pins the behavior.
- [Chains already broken before first archive write can never be archived] → the archive can only be built while history is still fetchable. Upgrade urgency is real: ship before workspaces age. Document as a hard edge; the migration note below is the mitigation.
- [Archive bloat on the author's PDS blob quota] → segmentation bounds single blobs; superseded heads' archives become garbage-collectable once their records are no longer referenced. Total live footprint is one archive per workspace head.
- [A hostile head author ships a divergent archive] → caught: recomputed CIDs must match pins anchored at the verified head; divergent bytes fail closed and the verifier degrades to the live walk.
- [New dependency surface in core (`serde_ipld_dagcbor`, `cid`, `multihash`)] → all WASM-compatible, no-std-adjacent, no I/O; the dependency is justified by a protocol contract (byte-binding), which is what earns core placement.

## Migration Plan

No flag day. `chainArchive` is optional; readers ignore it when absent. A chain upgrades the first time any member supersedes after the feature ships — the author walks live (all hosts must still be alive for this one walk), builds the first archive, and every subsequent supersede extends it. Workspaces that never supersede again never gain an archive, and lose nothing they had. Rollback is trivial: readers that predate the field, or builds with the path disabled, fall back to the live walk unconditionally.

## Open Questions

- Should the maintenance daemon (`packages/opake-daemon`) proactively trigger a no-op curatorial supersede on archiveless workspaces to capture history while hosts are still alive, rather than waiting for an organic membership event? Authority says manager-only; a daemon acting for a manager identity could. Deferred to the tasks discussion — it changes urgency, not design.
- Segment threshold value (proposal: 32 MB soft cap — comfortable margin under the 50 MB default, no PDS-config probing). Needs a number in the spec delta.
- Whether the frontier cache (#69) should record archive-verified nodes identically to live-verified ones (no reason it shouldn't — a verified fact is a verified fact — but the spec should say so to prevent divergent trust tiers).
