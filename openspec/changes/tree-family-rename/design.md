# Design: tree-family-rename

## Context

Pure mechanical rename; no engineering. The only design content is why the family is flat-with-prefixes and how the rename lands without a red window.

## Decisions

**1. Prefixes, not nesting.** Verified empirically: the openspec CLI resolves exactly `specs/<capability>/spec.md` — a spec at `specs/parent/child/spec.md` registers as capability `parent` with zero requirements, silently. spec_lint's citation grammar (`[a-z0-9-]+`) has no path separator either. Hierarchy therefore lives in names: `tree-chains`, `tree-topology`, `tree-cabinet` cluster in listings and read as one shelf.

**2. Rename and citation sweep are one commit.** spec_lint checks citations against canon headings on every run; a rename commit without the sweep (or vice versa) is a broken intermediate state. The sweep is grep-mechanical because requirement headings don't change — only the capability tag does.

**3. Content is byte-identical except the title line.** Any wording improvement, including the cabinet scope-out narrowing, belongs to cabinet-tree-and-cycle-rejection. Keeping this change semantically empty is what makes it safe to land immediately.

## Risks / Trade-offs

- [Prose mentions are not lint-guarded] → the linter only checks `spec:` tags and `cite()` calls; plain-English "directory-chains spec" pointers in other canon specs and docs/ need a manual grep sweep. Task 2.3 owns it; a leftover prose mention is cosmetic, not ledger-corrupting.
- [In-flight change references the new name] → cabinet-tree-and-cycle-rejection's deltas already say tree-chains; if this change somehow landed second, its sibling's delta prose would briefly name a not-yet-renamed spec. Sequencing note in both proposals covers it.

## Migration Plan

One commit; rollback is reverting it.

## Open Questions

None.
