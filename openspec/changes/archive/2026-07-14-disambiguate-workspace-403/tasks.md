## 1. Indexer response contract

- [x] 1.1 Replace `is_member?/2` with a three-valued membership resolution in `record_queries.ex` (no head / head-without-caller / member role) and delete the boolean helper — the workspace controller is its sole production caller; migrate its tests to the new resolution
- [x] 1.2 Map the three states in `workspace_controller.ex`: no head → 404 `{"error": "workspace_not_indexed"}`, head without caller → 403, member → serve; applies to snapshot, sync, and chain-head
- [x] 1.3 Controller tests pinning the split: pre-genesis workspace → 404 + code; non-member of indexed workspace → 403; torn-down workspace → 404 + code; member → 200. Cite `indexer-consistency § Unknown workspace is distinguishable from non-membership`

## 2. Client retry narrowing

- [x] 2.1 Locate the current visibility-gap tolerance in the WASM/SDK chain-head resolution and workspace-scoped fetch paths; classify `workspace_not_indexed` (by body code, not status integer) as retryable-within-window
- [x] 2.2 Make 403 terminal: surfaces immediately as an authorization error, never consumes the retry window; error types distinguish visibility-wait exhaustion from authorization denial
- [x] 2.3 Unit tests for the classification and both error surfaces. Cite `indexer-consistency § Dependent operations tolerate the visibility gap`

## 3. E2E and regression net

- [x] 3.1 Remove the save-retry-on-403 pattern from the federation-tier/web e2e suites; the client's conforming retry replaces it
- [x] 3.2 Federation-tier test: creator mutates a fresh workspace immediately after creation and succeeds without test-side retries; window exhaustion path asserted at unit level. Cite the amended requirement's scenarios
- [x] 3.3 Federation-tier test: authorization denial is not retried (non-member gets an immediate 403 error surface)

## 4. Docs and spec hygiene

- [x] 4.1 Update the endpoint table in `docs/indexer.md` and the workspace-scoped-endpoints line in `apps/indexer/CLAUDE.md` with the 404/403 contract
- [x] 4.2 Run `just spec-lint` and confirm the ledger: new requirement cited, amended requirement's citations still resolve, 0 dangling
