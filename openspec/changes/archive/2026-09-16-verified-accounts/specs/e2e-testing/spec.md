## MODIFIED Requirements

### Requirement: CLI federation tier covers cross-PDS scenarios

The CLI e2e harness SHALL support a dev-env mode, selected by environment, in which tests exercise scenarios spanning multiple PDSes and the indexer — including workspace membership across PDSes and self-removal (leave). Federation specs SHALL run only in dev-env mode.

A scenario that wraps a key to a counterparty resolving as unverified SHALL supply the confirmation the operation now requires (`spec:account-verification § Wrapping a key to an unverified account requires explicit confirmation`). The harness SHALL expose an explicit affordance for supplying it, per operation, and SHALL NOT rely on a default, a suppressed prompt, or a global test-mode bypass: the confirmation is behaviour under test, so a harness that removes it stops exercising the requirement and hides its regression. Since the dev-env's fixtures are unverified apart from the anchored actor (`spec:dev-env § Deterministic actor fixtures`), a federation scenario that omits the affordance SHALL fail on the missing confirmation rather than proceed.

Federation specs SHALL cover all three resolution outcomes across PDSes: an operation to a verified counterparty proceeding without a prompt, an operation to an unverified counterparty proceeding only under the supplied confirmation, and an operation to a counterparty in the error state being refused or the counterparty excluded (`spec:account-verification § Recipients are resolved independently and a multi-recipient operation never fails wholesale`).

#### Scenario: hermetic leave smoke test

- **WHEN** two fixture actors on different PDSes share a workspace and the non-owner runs `opake workspace leave` in dev-env mode
- **THEN** the leave supersede is indexed, the leaver's workspace list no longer contains the workspace, and the remaining member's membership view reflects the departure — with no live accounts involved

#### Scenario: a cross-PDS add to an unverified actor supplies its confirmation

- **WHEN** a federation spec adds an unverified fixture actor on another PDS to a workspace
- **THEN** the spec supplies the confirmation through the harness's explicit affordance and the add completes

#### Scenario: an omitted confirmation fails the scenario

- **WHEN** a federation spec wraps a key to an unverified counterparty without supplying the confirmation
- **THEN** the operation is refused and the spec fails, rather than the harness answering on the caller's behalf
