---
name: spec-crossref-review
description: Semantic cross-spec impact review for an openspec change. Use after proposing or editing any change whose delta touches concepts other specs lean on (federation-class changes almost always do), or when the user asks whether a spec change breaks sibling specs. Runs spec-lint for the mechanical half, then drives an Opus cowboy to hunt for semantic staleness the lint cannot see. Invoke as /spec-crossref-review <change-name>; with no argument, review every non-archived change under openspec/changes/.
---

# Cross-spec impact review

spec-lint (`just spec-lint`) checks the *mechanical* half of cross-spec
consistency: `spec:capability § Requirement` citations resolve, REMOVED
requirements aren't still cited, MODIFIED requirements print a blast-radius
note. What it cannot see is *semantic* staleness — a sibling spec whose
prose assumes behavior this change alters, without naming any requirement.
That is this skill's job, and it needs a real reviewer brain, so it runs
through a cowboy on Opus.

## Procedure

1. **Mechanical gate first.** Run `just spec-lint`. If it exits non-zero,
   stop — fix the dangling citations before burning an Opus pass on a
   change that already fails the cheap check. Capture any `note:` lines
   (MODIFIED blast radius); they are input for step 3.

2. **Assemble the review packet.** For the change under review:
   - The full delta specs: `openspec/changes/<change>/specs/*/spec.md`
   - `proposal.md` and `design.md` from the change dir, if present
   - The list of sibling capabilities: every dir under `openspec/specs/`
     that does NOT have a delta in this change
   - The spec-lint `note:` lines for this change

3. **Spawn ONE cowboy** — `Agent` with `subagent_type: "Opake Review"`,
   `model: "opus"`, foreground (`run_in_background: false`; do not use tmux
   teams — broken on this machine, see memory).

   **Harvesting the report:** the Opake Review agent has SendMessage
   (added 2026-07-27) and its definition instructs it to deliver the
   complete report — not a summary — via SendMessage to its spawner.
   Expect the full report as a message; the final assistant text in the
   agent result carries it as well. Fallback if neither arrives (e.g. a
   stale agent definition): extract the last assistant text from the
   agent's transcript JSONL under `~/.claude/projects/<project-dir>/`
   (grep the .jsonl files for a distinctive phrase from the brief).

   Prompt shape:

   > You are reviewing an openspec change for semantic cross-spec impact.
   > The change's delta specs are at <paths>. Read them fully. Then read
   > every sibling main spec under openspec/specs/ that has NO delta in
   > this change: <list>. Hunt for semantic staleness: prose in a sibling
   > spec that assumes behavior this delta alters — guarantees weakened,
   > scenarios that become false, invariants the delta breaks, lifecycle
   > assumptions invalidated — even when no requirement name is cited.
   > Also judge whether any sibling SHOULD have received a delta in this
   > change and didn't. Verify every claim against the spec text; quote
   > the exact stale sentence. Do NOT flag stylistic issues or restate
   > what spec-lint already checks. Return findings as a list:
   > `capability § requirement — quoted stale text — why the delta
   > invalidates it — suggested disposition (extra delta | open question |
   > false alarm risk)`. Return `CLEAN` with one sentence of reasoning
   > per sibling spec if nothing is stale.

   Require a verdict for **every** sibling, stale or not, so "nothing
   here" is something the reviewer wrote rather than something nobody
   noticed. Any suspected trouble spots you include are hypotheses to
   test, not conclusions to confirm — ask for them to be named explicitly
   when they do not hold. A pass that only finds what it was pointed at
   has confirmed your priors instead of checking them.

4. **Verify before surgery.** Findings that would cause real spec work —
   an extra delta, a requirement rewrite — are worth one refutation pass
   before acting on them. Spawn a second reviewer whose brief is to
   attack the findings, told plainly that "all of them hold and I found
   nothing new" is a complete answer. Default to refuted on uncertainty:
   unnecessary spec surgery costs more than a second opinion.

5. **Report, never auto-fix.** Findings go to Noï as a review readout.
   Canon specs and change deltas only change after her red pen — the
   disposition for a confirmed finding is either an additional delta in
   the same change or an Open Questions entry, her call. Do not edit
   `openspec/specs/` from this skill.

## When to run

- After `/opsx:propose` for any change touching workspace-identity,
  directory-chains, workspace-membership, document-crypto,
  sharing-grants, or keyring-tombstones — these six cite and assume each
  other heavily.
- Before `/opsx:sync` or `/opsx:archive` on a change that produced
  spec-lint MODIFIED notes.
- Skip for changes whose deltas only touch tooling/dev-env capabilities
  with no crypto or federation semantics; the Opus pass is not free.
