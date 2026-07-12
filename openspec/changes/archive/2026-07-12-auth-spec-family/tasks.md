# Tasks: auth-spec-family

## 1. Specs (red-pen gate)

- [x] 1.1 auth-identity delta reviewed and approved
- [x] 1.2 auth-session delta reviewed and approved
- [x] 1.3 auth-pairing delta reviewed and approved

## 2. Citation seeding (comment-only edits)

- [x] 2.1 `// spec:auth-identity § …` comments into crates/opake-crypto/src/mnemonic_tests.rs (determinism, golden vector, parse refusals, debug redaction) and tests/tests/cli/{login,recover}.test.ts cite() calls
- [x] 2.2 `// spec:auth-session § …` comments into crates/opake-core/src/client/{session_refresh_tests,oauth_token_tests,dpop_tests}.rs and crates/opake-core/src/scope.rs tests; tests/tests/cli/login.test.ts multi-account cite
- [x] 2.3 `// spec:auth-pairing § …` comments into crates/opake-core/src/pairing/{request_tests,receive_tests,cleanup_tests}.rs and tests/tests/cli/pairing.test.ts cite() calls
- [x] 2.4 Coverage ledger reviewed: uncited auth requirements are the intended honest set (substituted-response rejection, publish-on-login, unmount cancel), not accidental gaps

## 3. Remove the random-keypair JS surface

- [x] 3.1 Delete the `generateIdentity` WASM export (crates/opake-wasm/src/lib.rs) and SDK static (packages/opake-sdk/src/opake.ts); regenerate dist types. `Identity::generate` stays in core (test-helper caller only)
- [x] 3.2 Verify no web/SDK/test caller remains (`rg generateIdentity` clean outside dist history)

## 4. Doc corrections surfaced by the scout

- [x] 4.1 docs/AUTH.md identity file description includes ML-KEM-768 (currently "X25519 + Ed25519 keypairs")

## 5. Gates

- [x] 5.1 `just spec-lint` green (0 dangling)
- [x] 5.2 `just validate` green

## 6. Archive

- [x] 6.1 Sync deltas to canon (openspec/specs/{auth-identity,auth-session,auth-pairing}/spec.md)
- [x] 6.2 Archive the change
