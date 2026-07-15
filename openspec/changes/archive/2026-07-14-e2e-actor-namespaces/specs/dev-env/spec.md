# dev-env Delta

## MODIFIED Requirements

### Requirement: Deterministic actor fixtures

The dev-env SHALL provision a fixed set of named actors from checked-in BIP-39 mnemonics, with at least one actor on each PDS instance, so owner/member/third-party scenarios have a resident actor per role. Each bootstrapped actor SHALL have a published `at.opake.publicKey/self` record derived from its mnemonic. Fixtures SHALL NOT hardcode `did:plc` values; actors are addressed by handle and resolved at runtime.

Beyond the checked-in set, test harnesses SHALL be able to provision namespace-scoped actors on demand against a running dev-env. A namespaced actor's handle SHALL embed its namespace, its mnemonic SHALL derive deterministically from the namespace and actor role (no randomness), and its provisioning SHALL yield the same guarantees as bootstrap: a live account on the role's designated PDS and a published public-key record derived from the mnemonic. The checked-in fixture set is the default namespace; provisioning a namespace SHALL NOT mutate the default actors or any other namespace's actors.

A namespace SHALL be individually disposable: a deprovision operation removes that namespace's actors and their accumulated state (accounts, records, blobs) from the dev-env without touching the default actors, any other namespace, or the environment's lifecycle — full `reset` remains the pristine-baseline path, but SHALL NOT be the only cleanup available, since it destroys state shared with concurrent consumers.

#### Scenario: stable encryption identity across resets

- **WHEN** the dev-env is bootstrapped, reset, and bootstrapped again
- **THEN** each actor's published X25519 encryption public key is identical across both bootstraps

#### Scenario: every PDS is inhabited

- **WHEN** the default fixture set is bootstrapped
- **THEN** each PDS instance hosts at least one actor, and any actor can resolve any other actor's public key record

#### Scenario: namespaced actors derive deterministically

- **WHEN** the same namespace is provisioned twice against a freshly reset dev-env
- **THEN** each actor in the namespace has the same handle, the same PDS placement, and the same published encryption public key across both provisionings

#### Scenario: provisioning a namespace leaves other populations untouched

- **WHEN** a namespace is provisioned while the default fixture actors hold live sessions
- **THEN** the default actors' accounts, key records, and sessions are unchanged

#### Scenario: deprovisioning removes exactly one namespace

- **WHEN** a namespace whose actors have created workspaces and documents is deprovisioned while another namespace and the default actors exist
- **THEN** the deprovisioned namespace's accounts and records are gone, and the other namespace's and default actors' accounts, records, and sessions are unchanged
