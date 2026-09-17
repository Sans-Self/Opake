# auth-identity (delta)

## MODIFIED Requirements

### Requirement: Recovery re-derives and cross-checks the published key

Recovery (apps/cli/src/commands/recover.rs; web: apps/web/src/components/devices/RecoverIdentityView.tsx via apps/web/src/components/devices/useSeedPhraseRecovery.ts) SHALL derive the identity from the entered phrase and compare the derived X25519 public key against the account's published `publicKey/self` record. A mismatch SHALL NOT silently overwrite: the CLI warns and requires an explicit confirmation before saving; an absent published record is not a mismatch (fresh publish follows). Recovery SHALL refuse to run when a local identity already exists.

#### Scenario: recovered identity decrypts existing documents

- **GIVEN** an account whose documents were encrypted under the identity derived from phrase P
- **WHEN** a new device runs recovery with P
- **THEN** the re-derived identity decrypts the existing documents
- Verified end to end in "recover from plain text seed phrase → decrypt works" (tests/tests/cli/recover.test.ts); refusals in "recover rejects invalid seed phrase" / "when identity already exists" (tests/tests/cli/recover.test.ts)
