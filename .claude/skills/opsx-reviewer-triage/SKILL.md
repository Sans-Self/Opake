---
name: opsx-reviewer-triage
description: Walk an OpenSpec change's reviewer findings in severity order, propose the usual fix for mechanical ones, apply to the change's deltas on confirmation, route judgment findings to crossref.
allowed-tools: Bash(openspec-reviewer:*), Bash(openspec:*)
metadata:
  generatedBy: openspec-reviewer 0.1.0
  checksum: eaadbf258daebdaa
---
Walk a change's findings and fix the mechanical ones. This skill edits
only the delta files of the change under review,
`openspec/changes/<change>/specs/<capability>/spec.md`; it never edits
`openspec/specs/`, tests or source. Findings that need reading go to
`opsx-reviewer-crossref`.

## Steps

1. Run `openspec-reviewer change <change> --format json --no-state`.
   Keep the text of its `summary` for the end.
2. Present the findings grouped by severity, errors first, then
   warnings, then notes. For each, one sentence on what it means.
3. For each finding of a mechanical kind, propose the usual fix:

   | kind | usual fix |
   | --- | --- |
   | `citation_dangling` | the nearest canon requirement name, copied exactly |
   | `modified_without_canon` | move under ADDED, or fix the name |
   | `added_already_exists` | move under MODIFIED, or a new name |
   | `removed_without_canon`, `rename_source_missing` | fix the name to canon's |
   | `rename_target_taken` | a new name |
   | `requirement_without_scenario` | one scenario drafted from the body, one condition per keyword line |
   | `scenario_dropped` | restate the scenario, or a proposal sentence saying why it goes |
   | `unchanged_modified` | delete the entry |
   | `uses_deprecated_synonym` | the glossary term named in the message |
   | `new_term_undefined` | drop the backticks, or hand to `opsx-reviewer-define` |
   | `removed_still_cited` | a choice: keep the requirement, or update the citer in this change |

4. Show every edit as a diff before applying it. Apply only on
   confirmation, and only to files under `openspec/changes/<change>/`.
5. List the findings of kind `sibling_uses_removed`,
   `sibling_uses_old_name`, `term_in_use` and `modified_has_citers`
   under "needs reading" and name `opsx-reviewer-crossref` as the next
   step. Propose no edit for them.
6. Do not dismiss any finding. Dismissal is a key in the reviewer's
   view, for a person.
7. Run the review again and show the `summary` before and after.

## Depends on

- `review-findings § A finding has a severity, a location and a message`
- `review-findings § A modified requirement must exist in canon`
- `review-findings § A requirement without scenarios is a warning`
- `citations § A citation must resolve`
- `citations § Removing a cited requirement is an error`
