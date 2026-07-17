# verify-foreign-lineage

## Why

The workspace resolution paths trust a keyring record's self-declared `lineage` as the workspace identity without verifying it (#51). `resolve_foreign_workspace` fetches a keyring record from a stranger's PDS and adopts `lineage_anchor()` verbatim as `Workspace.uri` — the value every downstream consumer keys on: keeper entries, cache keys, the `workspaceId` stamped on uploaded documents.

An attacker can craft a fresh keyring on their own PDS declaring `lineage` set to a victim workspace's genesis URI, wrap the target's key and seal metadata under that declared anchor (all crypto succeeds — the AAD's lying-writer property covers tampering with existing chains, not fresh malicious records), and deliver it through the indexer's `keyring:upsert` fan-out to the members it lists. The resolved workspace then collides with the victim workspace's identity in every keyed store, and documents the target uploads label themselves as the victim's while encrypting under the attacker's group key.

Procedural fixes (verify-by-chain-walk at first contact, with a memo for steady state) were designed and cross-reviewed, and failed review: a walk depth cap contradicts `key-rotation`'s no-correctness-cliff guarantee for late joiners, a forward-only memo breaks under `keyring-tombstones` rollback, and a dead historical PDS turns an honest workspace unjoinable. The root cause is that a *declared* identity can only be checked by reconstructing history.

This change makes the identity *derived* instead: the genesis keyring's rkey is computed from the genesis group key. A declared lineage is then verified by one offline KDF — unwrap the group key you were handed, derive, compare against the rkey inside the declared URI. An outsider cannot mint a keyring that derives to a workspace identity whose rotation-0 key they do not hold; the impersonating record stops being rejectable and becomes unconstructible. No walk, no memo, no depth cap, no dependence on any historical PDS being alive.

A `supersedes` content pin (predecessor CID) rides along on all superseding record kinds. At v1 the reader compares it against the CID a host *reports* for the predecessor (clients do not recompute atproto CIDs yet), so it catches disagreement between honest hosts but not a hostile host serving tampered bytes under the true CID; recomputing the CID from bytes — real tamper-evidence — is deferred to the replication/archival work that needs it. The field is added now because the pre-v1 window makes it free. The pre-v1 carve-out (`spec:record-validity § opakeVersion is a stable protocol contract`) makes both wire changes free now; after v1 they become versioned migrations. This change declares that pre-v1 wire break: records written under the prior draft are garbage, development environments reset, no shim.

## What Changes

- Workspace creation derives the genesis keyring rkey from the genesis group key together with the owner's DID (HKDF → identity keypair → pubkey-hash tag, base32). The workspace identity — the genesis URI — thereby commits to both its segments: `at://<owner>/at.opake.keyring/<derived-tag>`, key-bound rkey, derivation-bound authority.
- Every identity adoption verifies by derivation: rotation-0 group key + the declared anchor's authority DID → tag → compare against the rkey of the lineage anchor. A forged rkey and a forged owner attribution fail the same offline check. The adopting paths are enumerated — direct resolution (distinct error on mismatch) and the `WorkspaceKeeper` bootstrap/`keyring:upsert` fan-out (silent drop on mismatch: a derivation-mismatch record is a forgery targeting this user, and rendering it in any form is the forger's payoff).
- The identity derivation's intermediates (HKDF seed, Ed25519 keypair) join the auto-zeroize regime; the private half is dropped immediately — nothing signs in this change. Both resolution branches (own and foreign) run the check — it is one KDF, so there is no cheap/expensive asymmetry to manage. Mismatch fails resolution.
- A record that declares `lineage` without `supersedes` is treated as outside any chain (client mirror of the indexer's naked-supersede rule), composing with the existing read-lenient skip posture rather than adding a new failure taxonomy.
- `supersedes` gains a companion content pin (`supersedesCid`, predecessor CID) on keyring, document, and directory records; writers stamp it per level, readers verify it when present; a pin mismatch on a directory walk is an unverifiable link feeding tree-chains' existing degrade-not-error posture.
- The rotation-0 group key becomes explicitly identity-load-bearing in key-rotation canon: it was already unprunable while referenced; it is now referenced by the workspace identity itself, permanently.
- Explicitly not built, with the reasoning on record: first-contact chain walks, verified-heads memos, and walk depth caps — the procedural design this replaces.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workspace-identity`: the genesis rkey is key-derived; every identity adoption (direct resolution and keeper surfaces) verifies by derivation, not declaration.
- `lineage`: the client-chosen-rkey rule admits the derived keyring-genesis tag; naked lineage (lineage without supersedes) is outside any chain; supersede references carry a content pin.
- `tree-chains`: directory records carry the pin; a pin-mismatched link is unverifiable and feeds the existing degradation posture.
- `key-rotation`: the rotation-0 group key is identity-load-bearing and permanently retained.
- `document-crypto`: the identity derivation's seed and keypair join the zeroize-on-drop regime.

## Impact

- `crates/opake-crypto/` — workspace identity tag derivation (HKDF through the shared transcript encoder; new label constant).
- `crates/opake-core/src/keyrings/create.rs` + `crates/opake-core/src/opake.rs` — creation flow derives the rkey before building the genesis record; both resolution branches verify by derivation.
- `crates/opake-core/src/records/{keyring,document,directory}.rs` + `lexicons/at.opake.{keyring,document,directory}.json` — `supersedesCid`; keyring record key type widens from `tid` to accept the derived tag; the document lexicon's stale "history annotation only" description of `supersedes` is corrected while the field is touched.
- `crates/opake-core/src/directories/chain.rs` — pin verification in the shared walk; mismatch classified as an unverifiable link.
- Writers that build superseding records (`opake.rs` advances, `directories/cascade.rs`, `manager/upload.rs`, `manager/editor.rs`) stamp the pin.
- Out of scope, deliberately: member chain mirrors and verifiable proof bundles (replication-class work, tracked toward #19); indexer-side provenance (#57 — the derivation is members-only by construction, the server cannot check it); document/directory identity derivation (per-record content keys have no stable chain key, and their AAD already fails closed on anchor lies).
