---
name: opsx-reviewer-define
description: Turn openspec-reviewer's recurring undefined terms into a drafted definitions delta in a new change. Writes under openspec/changes/ on confirmation.
allowed-tools: Bash(openspec-reviewer:*), Bash(openspec:*)
metadata:
  generatedBy: openspec-reviewer 0.1.0
  checksum: 60b46c631b150bc6
---
Turn the lint's recurring undefined terms into a drafted glossary. This
skill writes one file, `openspec/changes/<name>/specs/definitions/spec.md`
in a new change you name, and nothing else; it never edits
`openspec/specs/`. The reviewer's review of that change is the approval.

## Steps

1. Run `openspec-reviewer lint --format json`. Collect every finding
   whose message starts with `recurring term without definition`; the
   term is in backticks in the message and `details[]` lists the
   requirements that use it as `capability § requirement`.
2. Read each listed requirement in `openspec/specs/<capability>/spec.md`.
3. Sort the candidates. Keep a term when the requirements use it as a
   concept the reader has to know: a role, a key, a record, a state.
   Leave a candidate undefined when it is a field name, a path, a
   command, a JSON key or a type name: the code defines those. When in
   doubt, leave it out and say so in your report.
4. Create the change: `openspec new change <name>` with a name that says
   what it defines, then write the delta with one entry per kept term:

   ```markdown
   # definitions (delta)

   ## ADDED Requirements

   ### Requirement: <term>

   A spec MUST use `<term>` to mean:

   <one or two sentences distilled from how the requirements use it>

   - **Admitted:** <other words the project is content to keep saying>
   - **Deprecated:** <words the project should stop saying>

   #### Scenario: In a sentence

   - **WHEN** <lifted from a real requirement>
   - **THEN** <lifted from the same requirement>
   ```

   The first line is the binding line and every term has one, spelled
   exactly that way, naming the requirement. It is what the entry
   asserts — that this word means one thing everywhere — and without it
   `openspec validate` rejects the term for holding no SHALL or MUST.

   Sort each near-synonym the uses show onto one line or the other: a
   word the project is content to keep saying is admitted, a word it
   should stop saying is deprecated. Never put a word on both lines.
   Drop either line when no word belongs on it. Lift the scenario from a
   requirement that uses the term, rewording only to fit the two keyword
   lines.
5. Show the draft and the list of candidates you left out, with one
   reason each, before writing anything.
6. Run `openspec-reviewer change <name> --format json --no-state` and
   report its summary. A `defined_but_unused` note on a term you just
   drafted means the uses were all in deltas, not canon; say so.

## Depends on

- `glossary § The glossary is a capability named definitions`
- `glossary § A term lists the words not to use for it`
- `glossary § A term lists the words that are acceptable for it`
- `glossary § A recurring undefined term is a note`
- `glossary § A term nobody uses is a note`
