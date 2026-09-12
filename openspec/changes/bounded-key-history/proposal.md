## Why

Finding R3 exposes a mismatch between finite record arrays and indefinitely retained key
history. An unswept workspace must not become unwritable merely because another rotation
or admission exceeds an embedded snapshot's size limit.

## What Changes

- Keep the live keyring head and individual history records size-bounded; place historical
  key material in separately addressable records rather than an ever-growing head array.
- Locate historical keys by rotation without sequentially walking unrelated rotations.
- Preserve rotation-0 identity material permanently and retain all still-referenced generations.
- Publish the history needed by a new head synchronously before or atomically with that head.
- Accept an initial limit of 256 simultaneous members, not 256 lifetime recipients per generation.
- **BREAKING**: replacing embedded history is a storage-format change, with a coordinated pre-v1
  deployment/reset plan rather than a silently compatible optional field.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `key-rotation`: bounded history storage, synchronous publication, and historical admission.
- `workspace-membership`: the simultaneous-member limit and storage-independent removal history.
- `document-crypto`: rotation-selected reads through separately stored history.
- `workspace-identity`: genesis verification obtains rotation 0 without requiring an embedded array.
- `record-validity`: explicit per-record bounds and the history-format break.
- `background-work`: retained-history growth is storage debt, not an obligatory linear history walk.

## Impact

This is a separate protocol/storage design, not a larger integer in a lexicon. It layers after
`verified-accounts` and `rotation-write-safety` where their full replacement requirements overlap;
the change map records the sync order. Review and benchmark the wire layout, authenticated
lookup, historical admission, and custody/replication before implementation. Those mechanisms
have not been selected merely by accepting the 256-member product limit.

Affected surfaces include lexicons, record validation, history reads, admission/rotation,
indexer discovery, identity adoption, and fixtures. No PDS byte ceiling was measured in the
scenario review. This change must not introduce an authority-history walk; it also does not
claim to remove the existing one or solve availability after every holder/host disappears.
