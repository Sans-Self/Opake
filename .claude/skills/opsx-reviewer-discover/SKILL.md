---
name: opsx-reviewer-discover
description: Find the vocabulary a repository's specs rely on in plain prose, across capabilities, and hand the candidates to define. Writes nothing itself; define writes under openspec/changes/ on confirmation.
allowed-tools: Bash(openspec-reviewer:*), Bash(openspec:*)
metadata:
  generatedBy: openspec-reviewer 0.1.0
  checksum: 491dbcae5b0280af
---
Find the vocabulary a repository relies on without marking it up. The
lint's "recurring term without definition" note sees backticked and
quoted spans only; this skill reads the prose too, and hands what it
finds to `define`, which writes `openspec/changes/<name>/specs/definitions/spec.md`
in a new change. This skill writes nothing itself and never edits
`openspec/specs/`.

## Steps

1. Run `openspec-reviewer lint --format json` and collect every finding
   whose message starts with `recurring term without definition`, with
   its `details[]`. These are the span candidates.
2. Read every `openspec/specs/<capability>/spec.md`. For each
   `### Requirement:` heading, take the noun phrases of one to three
   words from the name and from the body, ignoring the keyword lines
   and anything already backticked. Record each phrase with the
   `capability § requirement` it appears in.
3. Keep a phrase only when it appears in two or more capabilities. A
   spec repeats its own subject; recurrence inside one capability is
   not a signal. Drop the rest without listing them.
4. Read `openspec/specs/definitions/spec.md` if it exists. Drop every
   phrase that is a term there, or sits on a term's `- **Admitted:**`
   or `- **Deprecated:**` line. The glossary already accounts for those.
5. Merge the surviving phrases with the span candidates from step 1.
   Where a phrase and a span name one concept in two spellings, keep
   one entry and note the other as a synonym for `define` to sort.
6. Show the list ordered by how many capabilities each entry appears
   in, with its uses. Ask how many to define in this change; twenty is a
   change a reviewer can read.
7. Hand the chosen entries to `define` from its step 3 onward, as
   candidates with their uses, into a change named for what it defines.
8. Run `openspec-reviewer change <name> --format json --no-state` on
   the change `define` wrote and report its summary. Entries you left
   out stay in your report with their capability count, so the next
   batch starts where this one stopped.

## Depends on

- `glossary § The glossary is a capability named definitions`
- `glossary § A recurring undefined term is a note`
- `glossary § A term lists the words that are acceptable for it`
- `skills § The define skill drafts glossary terms`
- `change-model § Canon is every spec under the specs directory`
