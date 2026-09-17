# dev-env (delta)

## MODIFIED Requirements

### Requirement: Deterministic actor fixtures

The dev-env SHALL provision a fixed set of named actors from checked-in BIP-39 mnemonics, with at least one actor on each PDS instance, so owner/member/third-party scenarios have a resident actor per role. Each bootstrapped actor SHALL have a published `at.opake.publicKey/self` record derived from its mnemonic. Fixtures SHALL NOT hardcode `did:plc` values; actors are addressed by handle and resolved at runtime.

At least one bootstrapped actor SHALL be verified: its published record carries a `signature` over the signed transcript (`spec:account-verification § The signature covers a fixed, versioned transcript that names the account`), and its DID document in the dev-env PLC directory carries the matching `#opake` verification method. Bootstrap SHALL publish the signed record before the verification method, in the order an account becoming verified is required to use (`spec:auth-identity § The encryption public keys are published as the publicKey self-record`). The remaining fixtures SHALL stay unverified, so the unverified path is the environment's default rather than a special case that has to be constructed.

All three resolution outcomes SHALL therefore be reachable in the hermetic environment without a hostile component: unverified from any ordinary fixture, verified from the anchored one, and the error state by rewriting the anchored actor's published record so its signature is absent or no longer verifies while the verification method stays in place. The error state SHALL be producible against the anchored actor's own PDS through ordinary record writes; the dev-env SHALL NOT require a modified PDS to reach it (`spec:account-verification § Key resolution is three-valued, and an anchored account may not serve an unsigned record`).

Beyond the checked-in set, test harnesses SHALL be able to provision namespace-scoped actors on demand against a running dev-env. A namespaced actor's handle SHALL embed its namespace, its mnemonic SHALL derive deterministically from the namespace and actor role (no randomness), and its provisioning SHALL yield the same guarantees as bootstrap: a live account on the role's designated PDS and a published key record derived from the mnemonic. The checked-in fixture set is the default namespace; provisioning a namespace SHALL NOT mutate the default actors or any other namespace's actors.

A namespace SHALL be individually disposable: a deprovision operation removes that namespace's actors and their accumulated state (accounts, records, blobs) from the dev-env without touching the default actors, any other namespace, or the environment's lifecycle — full `reset` remains the pristine-baseline path, but SHALL NOT be the only cleanup available, since it destroys state shared with concurrent consumers.

#### Scenario: stable encryption identity across resets

- **WHEN** the dev-env is bootstrapped, reset, and bootstrapped again
- **THEN** each actor's published X25519 encryption public key is identical across both bootstraps

#### Scenario: every PDS is inhabited

- **WHEN** the default fixture set is bootstrapped
- **THEN** each PDS instance hosts at least one actor, and any actor can resolve any other actor's published key record

#### Scenario: the anchored fixture resolves as verified

- **WHEN** any fixture actor resolves the anchored actor's keys against the dev-env PLC and PDSes
- **THEN** resolution yields the verified state, the record's signature having verified under the `#opake` verification method

#### Scenario: the error state is reachable without a modified component

- **GIVEN** the anchored fixture actor after bootstrap
- **WHEN** its published record is rewritten with its signature removed and its verification method left in place
- **THEN** a counterparty resolving it yields the error state, using unmodified dev-env components throughout

#### Scenario: namespaced actors derive deterministically

- **WHEN** the same namespace is provisioned twice against a freshly reset dev-env
- **THEN** each actor in the namespace has the same handle, the same PDS placement, and the same published encryption public key across both provisionings

#### Scenario: provisioning a namespace leaves other populations untouched

- **WHEN** a namespace is provisioned while the default fixture actors hold live sessions
- **THEN** the default actors' accounts, key records, and sessions are unchanged

#### Scenario: deprovisioning removes exactly one namespace

- **WHEN** a namespace whose actors have created workspaces and documents is deprovisioned while another namespace and the default actors exist
- **THEN** the deprovisioned namespace's accounts and records are gone, and the other namespace's and default actors' accounts, records, and sessions are unchanged
