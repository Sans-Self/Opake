---
name: opsx-reviewer-workflow
description: How to work with openspec-reviewer in a repository that has an openspec/ directory and the openspec-reviewer binary: when to run the review and the lint, what each finding kind means, how to write a spec: citation, and which opsx-reviewer skill handles a finding that needs judgment.
allowed-tools: Bash(openspec-reviewer:*), Bash(openspec:*)
metadata:
  generatedBy: openspec-reviewer 0.1.0
  checksum: ee1f0d81057a2089
---
Use this skill in any repository with an `openspec/` directory and an
`openspec-reviewer` binary. It tells you when to run the reviewer, how to
read what it says, and which task skill to load when a finding needs
more than a mechanical fix. This skill writes nothing itself: `define`
writes a new change, `cite` edits test titles, `crossref` and `triage`
edit the deltas of the change under review. Nothing you do from here
edits `openspec/specs/`; canon changes only by archiving a change.

## When to run what

- After editing any file under `openspec/changes/<name>/`, and again
  before you propose archiving the change, run
  `openspec-reviewer change <name> --format json --no-state`.
- After editing source or tests that carry `spec:` citations, or a canon
  spec that names a path, a test or a commit, run
  `openspec-reviewer lint --format json`. If it refuses because
  `openspec/reviewer.toml` is missing, run `openspec-reviewer lint init`
  once and read the file it wrote before running the lint again.
- Exit status is the worst finding: `0` clean or notes only, `1` at
  least one warning, `2` at least one error.

## Reading a review

The JSON has `changes[].capabilities[].pairings[]`, one pairing per
requirement the change touches, with `kind` (`added`, `modified`,
`removed`, `renamed`), `before`, `after`, and `findings[]`. Each finding
has `kind`, `severity`, `location`, `message` and, when there is a list
to show, `details[]`. `summary` counts errors, warnings and notes. The
`definitions[]` array lists the glossary terms the change uses.

Fix errors before warnings. Read every note once; most tell you what
else the change touches.

### Errors

- `modified_without_canon`: the delta says MODIFIED but canon has no
  requirement of that name. Check the name against the canon spec; if
  the requirement is new, move it under ADDED.
- `added_already_exists`: the delta says ADDED but canon already has the
  name. Move the entry under MODIFIED, or pick a new name.
- `removed_without_canon`: the delta removes a name canon does not have.
  Fix the name or drop the entry.
- `rename_source_missing`: the RENAMED entry's old name is not in canon.
- `rename_target_taken`: the RENAMED entry's new name already exists.
- `citation_dangling`: a `spec:` citation in the requirement's text
  names a capability or requirement that does not exist. Use the canon
  name exactly; the message says which part failed.
- `removed_still_cited`: the change removes a requirement that a test or
  spec outside the capability still cites. Either keep the requirement
  or, in the same change, update the citer; load `crossref` to judge
  which.

### Warnings

- `scenario_dropped`: a MODIFIED entry lost a scenario canon has. Restate
  it or say in the proposal why it goes.
- `requirement_without_scenario`: draft at least one scenario from the
  body, one condition per keyword line.
- `cross_change_collision`: another open change touches the same
  requirement; read that change before continuing.
- `sibling_uses_removed`, `sibling_uses_old_name`: a canon requirement in
  another capability still uses a term or a name this change removes.
  Judgment, not a typo: load `crossref`.
- `uses_deprecated_synonym`: the text uses a word the glossary marks
  deprecated; the message names the term to use instead.
- `new_term_undefined`: the change introduces a backticked or quoted
  term twice with no glossary entry. Define it with `define`, or drop
  the backticks if it is not vocabulary.
- `term_without_binding_line`: a glossary term whose body does not open
  with ``A spec MUST use `<term>` to mean:``. Add the line, naming the
  requirement; without it `openspec validate` rejects the term.

### Notes

- `unchanged_modified`: a MODIFIED entry equal to canon; delete it.
- `history_unreadable`: an archive could not be parsed; the archive is
  named, leave it unless you own it.
- `modified_has_citers`: files outside the capability cite this
  requirement; `details[]` lists them. Read them before you change the
  meaning; load `crossref` if the change alters what they test.
- `defined_but_unused`: a glossary term nothing uses; leave it unless
  the change is about the glossary.
- `term_in_use`: the change edits a glossary term and `details[]` lists
  the requirements that use it. Load `crossref` to check each still
  holds.

## Reading the lint

`findings[]` carry `severity`, `file`, `message` and `details[]`. The
`summary` line at the end of text output counts what was checked; a
field named after `not configured:` is a check switched off in
`openspec/reviewer.toml`. Two messages have a skill of their own:
`recurring term without definition` is what `define` reads, and the
`--coverage` ledger of requirements with zero citing files is what
`cite` reads. A quiet lint over specs written in plain prose is what
`discover` is for: the span check cannot see a term nobody backticked.

## Writing a citation

A citation is `spec:<capability> § <requirement name>` in a test title,
a comment or a `cite()` call, inside double quotes or backticks. The
requirement name is copied from canon exactly, including capitals and
punctuation. When the citation wraps onto a second line, keep it inside
one backticked span; the comment marker on the continuation line is
ignored.

## Which skill

| finding | load |
| --- | --- |
| `recurring term without definition`, `new_term_undefined` | `opsx-reviewer-define` |
| no definitions notes over specs that name concepts in plain prose | `opsx-reviewer-discover` |
| uncited requirements in the coverage ledger | `opsx-reviewer-cite` |
| `sibling_uses_removed`, `sibling_uses_old_name`, `term_in_use`, `modified_has_citers`, `removed_still_cited` | `opsx-reviewer-crossref` |
| a change with several errors or warnings of mechanical kinds | `opsx-reviewer-triage` |

## Depends on

- `review-findings § A finding has a severity, a location and a message`
- `review-findings § Exit status reflects the worst finding`
- `citations § A citation names a capability and a requirement`
- `citations § The lint is a subcommand with a summary`
- `glossary § A recurring undefined term is a note`
- `term-drift § A removed term found in a sibling is a warning`
