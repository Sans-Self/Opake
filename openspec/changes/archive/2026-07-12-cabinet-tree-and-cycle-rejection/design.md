# Design: cabinet-tree-and-cycle-rejection

## Context

The change is mostly documentation (two new canon specs, the directory-chains → tree-chains capability rename, citation repoints); the one piece of engineering is lowering cycle refusal into `FileManager::move_entry`. The shared cycle invariant lives in tree-topology — a shape invariant belongs to neither write model, and filing it under either context spec was the same misfiling class the change exists to fix.

Current state: `check_cycle(tree, source_uri, target_dir_uri)` (crates/opake-core/src/directories/move_entry.rs) decides the invariant against a loaded `DirectoryTree`, but only the CLI calls it (apps/cli/src/commands/move_cmd.rs). The web duplicates the rule as UI logic (Move dialog disables the source and its descendants). `FileManager::move_entry` — the domain API both clients route through, and the only thing a raw SDK caller sees — never checks. Neither of its arms holds a `DirectoryTree`: the cabinet arm does two record fetches and an `applyWrites`; the workspace arm resolves paths against chain heads but never builds a tree.

## Goals / Non-Goals

**Goals:**

- A caller of `FileManager::move_entry` cannot write a cycle, in either context, regardless of what the client layer did or didn't check.
- No signature change to `move_entry`; no behavior change for the CLI and web happy paths.

**Non-Goals:**

- Indexer-side cycle validation (open question carried in tree-topology; separate change if pursued).
- Repairing an already-written cycle or hardening every tree consumer against malicious cyclic input (risk noted below; the domain-API guard prevents the honest-client case only).

## Decisions

**1. The manager derives descendant knowledge itself instead of taking a `DirectoryTree` parameter.**

Cycle condition: the move target is the moved entry itself or a directory inside its subtree. When the moved entry is a directory, `move_entry` walks the subtree downward from the entry — fetch its record, recurse into entries whose target URI parses to the directory collection — collecting directory URIs, and refuses if the target is among them (or equals the entry). Fetching goes through the arm's existing read path (`get_record` for cabinet, `fetch_chain_node` for workspace), so the walk sees the same records the move itself would.

- *Alternative — thread `&DirectoryTree` through the signature:* rejected. The CLI holds a tree, but the WASM/SDK path would have to marshal one across the boundary, and a caller-supplied tree reintroduces the trust problem the change exists to close (a stale or empty tree passes any check). The domain API's guard must be authoritative from its own reads.
- *Alternative — reuse `check_cycle` directly:* it stays as the CLI's early, offline check (better UX: refuses before any network). The manager-side walk is a sibling enforcement of the same invariant at write time; both cite the same requirement.

**2. Documents short-circuit the check.**

A document has no listing, so moving one can never create a cycle. The walk only runs when the moved entry's URI is in the directory collection — the common case (file moves) pays zero extra reads.

**3. Refusal is `Error::InvalidRecord` with the same message shape `check_cycle` uses,** so CLI users who bypass the early check see the identical error, and the web's existing error surface needs no new mapping.

**4. The web's missing-root fix delegates to core rather than reimplementing.**

Core's `ensure_root` is the single root-creation path (upload with no directory already runs it). The web fix removes the null-`rootUri` early returns and lets the write flow through with no directory URI, so root creation happens inside WASM on the same code path the CLI exercises. No JS-side root construction — the web never learns how a root is made, it just stops refusing to ask.

## Risks / Trade-offs

- [Extra reads on directory moves — one fetch per subtree directory] → acceptable: directory moves are rare and subtrees shallow in practice; document moves (the hot path) skip entirely.
- [TOCTOU: topology changes between the walk and the write] → cabinet is single-writer (own repo), so effectively none. Workspace: a concurrent supersede can race the check like it can race the move itself; existing fork detection covers the collision, and the indexer-side validation open question owns the adversarial case.
- [Tree consumers may not tolerate a cycle that got written anyway (hostile client)] → out of scope to fix here, but implementation SHOULD add one defensive unit test that `DirectoryTree`/`collect_descendants` terminates on cyclic input rather than hanging; if it doesn't terminate, that becomes its own ticket rather than silently shipping.

## Canon edit outside the delta grammar

The tree-chains cabinet scope-out bullet (a `## Non-requirements` item, unaddressable by ADDED/MODIFIED/REMOVED/RENAMED Requirements sections) narrows at sync time from "this spec covers workspace directory chains" to scoping out chain machinery only:

> Cabinet (personal, non-workspace) chain machinery. Cabinet trees use a `self`-rkey root and direct key wrapping with no supersede chain, additivity check, or fork handling; their structure and write semantics live in the tree-cabinet spec. Tree-shape invariants that hold in both contexts — cycle refusal at the domain API — live in the tree-topology spec, not here.

## Migration Plan

Purely additive guard; no data or wire changes. Deploy with the normal release; rollback is reverting the commit.

## Open Questions

- None beyond the two carried in the spec deltas (indexer-side cycle validation; recursive root deletion semantics). Both are deliberately excluded from this change's implementation scope.
