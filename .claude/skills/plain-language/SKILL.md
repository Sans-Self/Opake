---
name: plain-language
description: Use when writing or revising any openspec artefact — a requirement, a scenario, a proposal, a design doc, or canon under openspec/specs/. Covers the four plain-language principles the repo writes to, what they mean for a spec sentence and a requirement name, and how to rewrite prose that runs long.
---

# Plain language in specs

Every openspec artefact in this repo is written to ISO 24495-1:2023,
_Plain language — Part 1: Governing principles and guidelines_. The
standard is paywalled and copyright-protected, so nothing here
reproduces its text. What follows is this repo's reading of it, in our
own words, against our own artefacts. Read the standard itself through
your national standards body if you want the source.

The standard's four governing principles, in its order:

1. **Relevant** — readers get what they need.
2. **Findable** — readers can easily find what they need.
3. **Understandable** — readers can easily understand what they find.
4. **Usable** — readers can easily use the information.

All four are craft, not rules a tool applies. The sections below say
what each one looks like in an openspec artefact.

## Who the reader is

Plain language is defined against a reader, so name ours: the engineer
or reviewer who opens a spec to settle a question, having not read the
change proposal, six months after it was written. They are technically
fluent. They are not caught up on the argument that produced the
sentence they are reading.

That reader makes the register concrete. Domain vocabulary —
`Result`, `supersedesCid`, keyring chain, `did:plc`, group key — is
familiar and stays. What does not survive is the argumentative residue:
the aside defending a rejected alternative, the parenthesis inside a
parenthesis, the sentence that only parses if you remember the
discussion.

## Requirements

A requirement is read as law, one sentence at a time.

- **One rule per sentence.** If a sentence carries two SHALLs, it is two
  requirements or two sentences.
- **Subject first, then the modal, then the object.** "The client SHALL
  recompute the CID from the fetched bytes" — not "The CID SHALL be
  recomputed from the fetched bytes". Naming the actor is what makes a
  requirement testable.
- **State the behaviour, not the mechanism.** Mechanism belongs in code,
  where it is executable. (`openspec/config.yaml` says the same thing
  under `rules.design`.)
- **No argument in the requirement body.** Why the rule exists goes in
  `design.md`, or in a `> Note:` under the requirement. A requirement
  that spends three clauses fending off an objection has stopped being
  readable as law.

The hardest habit to drop is the em-dash clause that qualifies the rule
after it has been stated. One rule, ended with a full stop; the
qualification is its own sentence or its own requirement.

## Requirement names

A name is a citation key. Tests cite it (`spec:<capability> § <name>`),
sibling specs cite it, and `just spec-lint` resolves every one of those.
So a name is both a heading and an identifier.

- Write it as a short claim, not a topic: "Removal rotates the group
  key; leave does not", not "Group key rotation on removal".
- Plain words, no backticks, no paths, no code identifiers. A name with
  a symbol in it rots the moment the symbol is renamed, and takes every
  citing test with it.
- Keep it stable. Renaming a requirement is a breaking change to every
  citer, and `just spec-lint` will tell you exactly how many.

## Scenarios

Scenarios are where findability lives: a reader scanning for the case
they care about reads keyword lines, not prose.

- **One condition per keyword line.** A second condition gets its own
  `- **AND**` line. Never compound a WHEN with "and" — write two lines.
- **THEN states one observable outcome.** Further outcomes are further
  `- **AND**` lines. A THEN that lists three things is three lines.
- Keep each line short enough to read without scanning back. If a
  keyword line needs a subordinate clause to make sense, the scenario is
  carrying setup that belongs in GIVEN.

## Proposals

A proposal is read by someone deciding whether to fund the work.

- Lead with what changes, not with the history that led here.
- Say what is out of scope. Naming the boundary is what makes the
  proposal usable; a reader cannot infer it.
- Every claim about current behaviour carries a citation
  (`` `symbol` (path.rs) ``). An uncited claim is not plain, it is
  unverifiable — which is the same failure one layer down.

## Design docs

Design docs keep the engineering register. They are written for a
reader weighing alternatives, and the trade-off vocabulary is the
subject matter, not jargon layered over it.

The principles still bind the shape: headings that predict what follows,
one decision per section, the conclusion before its defence. What
relaxes is the sentence-level plainness, because a design doc's reader
has opted into the argument.

## Length, as a proxy

Length is the one property of plain prose a machine can see, so it is the
proxy worth holding yourself to. The shape of spec prose that reads well:

| Unit                  | Reads well | Getting long | Rewrite |
| --------------------- | ---------- | ------------ | ------- |
| Requirement sentence  | ≤ 25 words | 30           | 40      |
| Scenario keyword line | ≤ 15 words | 25           | 35      |
| Requirement name      | ≤ 6 words  | 8            | 12      |

Count a backticked span as one word: a citation-dense sentence is not
verbose, it is evidenced.

These are a floor under the worst case, not a target. A 39-word sentence
clears every number above and still fails the reader. Nothing in this
repo checks them today — `just spec-lint` is referential only. Until
something does, the numbers are for you and for review.

One thing worth knowing if you are tempted to automate it: "never
compound a WHEN with and" cannot be linted. Across the canon at the time
of writing, 326 of 872 keyword lines contain "and". Some are ordinary
noun phrases — "the verification method and the signed record". Many
are genuine compound conditions that predate this skill. No pattern
separates the two. That rule is read, not run.

## Rewriting a sentence that runs long

Splitting on the nearest comma usually produces two bad sentences. In
order:

1. **Find the rule.** One sentence states it. Everything else in the
   flagged sentence is either a second rule, a justification, or an
   example.
2. **Give the second rule its own sentence** — or its own requirement,
   if it has its own scenarios.
3. **Move the justification** to `design.md` or a `> Note:`.
4. **Move the example** into a scenario, where a reader looks for
   examples anyway.

If nothing survives step 1, the sentence was not a requirement.
