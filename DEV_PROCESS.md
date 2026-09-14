# Implementation retrospective: verified accounts

This was a large protocol change disguised as one feature, and the implementation
process took longer than it needed to. Both facts matter.

"Verified accounts" reached through cryptographic signatures, DID resolution,
temporary OAuth authority, PLC mutations, membership semantics, historical
decryption, background jobs, the indexer, native clients, WASM, and the web UI.
The 95 tasks were not simply implementation steps: they crossed several trust
boundaries. A green unit test could not establish that the actual PDS signed the
intended operation and that the PLC directory accepted it.

The hardest part was establishing trustworthy evidence. Several apparently
successful checks proved less than expected:

- A scope-checking regular expression always returned `false`, making credential
  isolation look correct.
- One agent inferred success from missing failure artifacts. Absence of an
  artifact is not evidence that the intended action occurred.
- A browser test tried to authenticate a member while that member's public-key
  signature was deliberately corrupted.
- An approval test changed a public encryption key without possessing its
  corresponding private key, then expected successful decryption.
- Individual Playwright runs owned a shared server, so completing one run could
  interrupt another.

Those were reasoning and orchestration mistakes, not unavoidable costs of the
cryptography. They were found, but too often after an expensive test cycle rather
than before it.

Delegation helped with the volume of implementation, but it created an integration
burden. A local slice could be complete while a shared DTO, fixture assumption, or
server lifecycle still broke the complete path. The implementation should have
started with tighter boundaries and stronger acceptance criteria. There is no
evidence here that model choice caused the delay; no controlled comparison was
run.

Operationally, the work shifted from implementing the specification to deciding
which observations could be trusted. That made percentage updates poor time
estimates: 93% of checklist items complete did not mean 7% of the effort remained.
The remaining work held much of the integration uncertainty.

If this process is repeated, make three changes:

1. Prove one complete path first: real OAuth, signed public record, PDS signer,
   PLC write, then a fresh DID read. Establish the interfaces and test environment
   before broadening the implementation.
2. Define evidence before delegating: expected records, scope checks with positive
   controls, decryption by a real recipient private key, explicit process exits,
   and repeatable fixtures.
3. Keep shared infrastructure centrally owned: one persistent browser server,
   isolated actor namespaces, and deliberate scheduling for tests that mutate
   shared state.

The resulting evidence is substantially stronger than a compiling codebase. The
process still took avoidable loops to get there. More capable models can reason
through those loops, but sound engineering process should prevent many of them.
