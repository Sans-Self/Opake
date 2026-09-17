---
name: opsx-reviewer-crossref
description: Judge the sibling requirements an OpenSpec change puts in question, quote archived decisions, and draft MODIFIED entries into the change on confirmation.
allowed-tools: Bash(openspec-reviewer:*), Bash(openspec:*)
metadata:
  generatedBy: openspec-reviewer 0.1.0
  checksum: 7e07807b98eb80e7
---
Judge the siblings a change puts in question. This skill edits only the
delta files of the change under review,
`openspec/changes/<change>/specs/<capability>/spec.md`, adding MODIFIED
entries on confirmation; it never edits `openspec/specs/`.

## Steps

1. Run `openspec-reviewer change <change> --format json --no-state`.
   Collect every pairing that has a finding of kind
   `sibling_uses_removed`, `sibling_uses_old_name`, `term_in_use`,
   `modified_has_citers` or `removed_still_cited`. Count the warnings in
   `summary`; you will report that number again at the end.
2. For each pairing, read its `before` and `after` text in full, and
   then the full text of every sibling the findings name: the sibling's
   `location` or `details[]` gives the capability, the requirement and
   the file.
3. Give every sibling one verdict:
   - `consistent`: what the sibling asserts still holds after the
     change, even if it uses the old word.
   - `contradicts`: the sibling asserts something the change removes or
     reverses.
   - `stale term`: the sibling's claim holds but the word it uses is the
     one the change removes or renames.
4. Search `openspec/changes/archive/*/design.md` and `proposal.md` for
   the pairing's capability name and for each removed or renamed term.
   Quote any passage that decided the question this change reopens,
   with the archive directory's name. When the change reverses a
   recorded decision, say so in one sentence; do not judge whether the
   reversal is right.
5. For each `contradicts` and `stale term` sibling, draft a MODIFIED
   entry: the sibling's full requirement, restated with the new term or
   the corrected claim, every scenario kept. Place it under
   `## MODIFIED Requirements` in the change's delta for the sibling's
   capability, creating that delta file if the change has none. Show
   each draft and write it only on confirmation.
6. Run the review again. Report the warning count before and after and
   name every warning that remains, each with its verdict, so the
   reader knows which are theirs to decide.

## Depends on

- `term-drift § A removed term found in a sibling is a warning`
- `term-drift § A sibling the change already touches is not a finding`
- `term-drift § An old requirement name in prose is a warning`
- `glossary § Changing a term lists the requirements that use it`
- `citations § Modifying a cited requirement lists its citers`
