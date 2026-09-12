## Why

Finding R2 distinguishes fresh post-removal encryption from ciphertext already prepared
under an old key. Rotation cannot retract published old-key ciphertext, and re-wrapping a
known content key cannot protect newly encrypted metadata from its former holders.

## What Changes

- State confidentiality in terms of fresh content keys protected by the new group key, not
  a universal wall-clock cutoff across PDSes.
- Refresh workspace authority and rotation before preparing writes; do not knowingly publish
  with a superseded rotation or silently fall back to a historical key.
- Use fresh content keys for changed content or metadata when prior holders were removed.
- Explicitly accept already-encrypted/in-flight old-key exposure; retry cannot undo disclosure.
- Keep rotation itself free of blob re-encryption and leave untouched historical content alone.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `document-crypto`: write freshness, changed-metadata confidentiality, and the in-flight boundary.
- `key-rotation`: the precise confidentiality guarantee of a removal-triggered rotation.

## Impact

This is a write-path/security-contract change, not an account-verification feature. It can be
implemented independently; when combined with `verified-accounts`, historical-only clients
must obey the same refusal to write without the current key.

Affected surfaces include uploads, edits, renames, rotation tests, and security documentation.
The current file format shares one key between blob and metadata: a file metadata edit that
requires a fresh key must also re-encrypt that file's blob, or fail without publishing the
edit. No metadata-key split or automatic whole-workspace re-encryption is introduced.
