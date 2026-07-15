# AI-Assisted Development Methods — Research Dossier (2026-06-09)

Three parallel research strands: crosslink + lineage, spec-driven tooling landscape, practitioner experience reports. Raw findings preserved below; synthesis discussed in-session. Working doc, untracked (SSE_AUDIT.md convention).

---

## Strand 1: Chainlink / Crosslink (Doll)

> Corrected 2026-06-09: first research pass misattributed the tool to an anonymous biotech. Noï supplied the real provenance; second pass confirmed it.

### Author and lineage
- **Author: Doll** (it/its) — GitHub `dollspace-gay`, Bluesky/atproto `dollspace.gay` (DID `did:plc:dzvxvsiy3maw4iarpvizsj67`). Self-bio: "builds tools for developers and the decentralized web. Rust systems programmer, AI tooling architect, and AT Protocol contributor." Patreon: "Doll's AI Tooling Development."
- **chainlink** — https://github.com/dollspace-gay/chainlink — Doll's personal tool, created 2025-12-27, Rust, crates.io `chainlink-tracker`, `.chainlink/issues.db`, MIT. Still maintained.
- **crosslink** — https://github.com/forecast-bio/crosslink — confirmed **GitHub fork of chainlink** (API: `parent: dollspace-gay/chainlink`), forked 2026-02-24. Doll is the #1 contributor, ahead of Forecast staff. **Forecast Bio (brain-health biotech) hired Doll for its work** (confirmed by Noï); crosslink is the productized multi-agent evolution (swarm, containers, web dashboard, SSH-signed agent identities, intervention tracking). The "regulated biotech audit" framing in the rule files is Forecast's compliance context on top of Doll's design.
- Describes itself as "beads-like" (Steve Yegge's beads, ~18.7k stars — the markdown-plans-fail argument: prose not structured data, non-queryable, bit-rots because agents don't update plans).

### Core loop
1. `quick "task"` — issue + session in one step
2. Agent works, leaving **typed comments** (`--kind plan|decision|observation|blocker|resolution|result|handoff`); strict mode makes them mandatory at checkpoints. "Good enough for now" is only allowed if the *why* is recorded as `--kind decision`
3. `session end --notes` — handoff survives context compression/restarts; mandatory past ~30-40 messages
4. Epic → Issue → Sub-issue ("**Beads**" — atomic units, single-session-completable; the **500-line rule** forces decomposition)

### Enforcement (hooks that can't be talked past)
- `work-check.py` (PreToolUse): strict mode **blocks** Write/Edit/Bash with no active issue
- Git mutation tiers: chainlink blocks commit/push/merge/rebase/reset in ALL modes ("human-only operations"); crosslink softens commit to **gated** (allowed with active issue), rest stay hard-blocked
- `prompt-guard.py`: full rules injected on first prompt (~15-30KB), then **adaptive drift detection** — silent until the agent drifts N turns, then a ~500B condensed reminder
- `post-edit-check.py`: stub/placeholder detection after edits
- Rule files `.chainlink/rules/*.md` (global/quality/rigor/project + 20+ per-language, auto-injected; priority Security > Correctness > Workflow > Style). Editing a rule = one-off correction becomes a standing rule. Ships elixir, elixir-phoenix, rust, typescript, typescript-react. `rigor.md` crypto section (fresh nonces, AEAD, X25519/AES-256-GCM, audited libs only) maps ~1:1 onto Opake's threat model
- **Memory→issue translation ritual** (strict rules): read MEMORY.md first, translate plan-of-record into issues/subissues with verbose comments quoting plan + acceptance criteria; "never let them drift apart"
- **Intervention tracking** (crosslink): `intervene <id> --trigger <type> --context` — human corrections as structured audit events; `workflow trail` = chronological rationale view

### VDD — Verification-Driven Development
Source: https://gist.github.com/dollspace-gay/45c95ebfb5a3a3bae84d8bebd662cc25
- **Builder** (Claude) vs **Adversary** ("Sarcasmotron", Gemini) in iterative friction; human arbitrates "whether Claude is bullshitting or Sarcasmotron is bullshitting"
- Adversary gets a **fresh context window every turn** (prevents "relationship drift"/sycophancy) + negative prompting, zero tolerance
- Phases: decomposition (chainlink beads) → initial verification (tests + human) → adversarial "roast" → feedback integration
- **Zero-Slop termination**: loop exits when the Adversary's critiques become detectably *hallucinated* — it has to invent problems → maximum viable refinement reached
- Modes: **advisory** (single-pass review, findings injected) vs **blocking** (full loop)
- Formal verification in CI: Kani (model checking), Wycheproof (crypto)

### VSDD — Verified Spec-Driven Development (the SDD-relevant one)
Source: https://gist.github.com/dollspace-gay/d8d3bc3ecf4188df049d7a4726bb2a00
Fuses SDD (contract-first) + TDD (red-green-refactor) + VDD (adversarial hardening) as sequential gates in one pipeline.

**Six phases:**
1. **Spec Crystallization** — human + Builder write: behavioral contract (pre/postconditions, invariants), edge-case catalog, and a **Verification Architecture**: a *Purity Boundary Map* (deterministic core vs effectful shell decided at spec time), property specifications (Kani/Dafny/TLA+), tool selection before implementation. Spec itself undergoes adversarial review. All spec items become chainlink beads
2. **Test-First Implementation** — all tests before code; Red Gate (verify tests fail); minimal code to pass; human approves test suite for spec-intent alignment
3. **Adversarial Refinement** — Sarcasmotron roast: spec fidelity, test quality, security surface, spec gaps revealed by implementation
4. **Feedback Integration** — critique routed by level: spec flaw → Phase 1; test flaw → 2a; impl flaw → 2c; new edge case → spec update + failing test
5. **Formal Hardening** — proofs, fuzzing (cargo-fuzz/AFL++), Wycheproof/Semgrep CI gates, mutation testing, purity-boundary audit
6. **Convergence — four-dimensional Zero-Slop exit**: spec critiques reduce to wording nitpicks AND no meaningful untested scenarios (high mutation kill rate) AND adversary invents implementation problems AND all formal properties pass. Exit only when all four survive simultaneously

**Roles:** Architect (human — strategic vision, spec approval, dispute arbitration) / Builder (Claude) / Tracker (chainlink) / Adversary (fresh-context hostile model).
**Traceability chain:** every line of code → test → verification property → spec requirement → bead.
**Acknowledged limits:** specs are hypotheses, not airtight gates; formal verification applies to ~5–10% of a codebase (critical invariants); feedback loops generate ceremony under delivery pressure; token cost proportional to rigor.

### Relevance assessment (this team)
- **We already carry chainlink DNA**: "crosslink comments as primary knowledge store", "don't narrate" (lifted near-verbatim from chainlink `global.md`), repo-root-only, memory discipline. The question is not adoption but re-engagement
- Typed `decision` trail + `workflow trail` = the comprehension bridge: reconstruct *why* without reverse-engineering diffs
- VSDD Phase 1 (spec crystallization w/ adversarial spec review) = the front-edge fix for design churn; the purity-boundary-at-spec-time idea matches opake-core's existing architecture instincts
- VDD advisory mode ≈ existing fresh-context review-cowboy habit, formalized; blocking mode + Wycheproof reserved for crypto-critical paths
- Gap: VDD/VSDD optimize *correctness assurance*, not *human comprehension retention* — zero-slop code is trustworthy without being explainable-by-the-human. Traceability chain helps reconstruction; investor-narrative layer still comes from strand 3 (doc walks, comprehension-as-gate)

### The essays (retrieved as atproto records — `site.standard.document` on Doll's PDS)
**"Beyond Gas Town: Why VDD-IAR is the Survival Strategy for 2026"** (`3mbrplpvipk2j`) — the methodology's positioning essay:
- Full name: **VDD-IAR — Verification-Driven Development via Iterative Adversarial Refinement**
- Framed explicitly *against* Yegge's Gas Town (agent-swarm factory, "definitely sloppy" by Yegge's own admission, desire-paths, software-as-disposable)
- Names the comprehension-debt disease directly: **"When code is moving too fast for human review, teams can quickly lose track of the 'source of truth'"**
- The adversarial spiral: Builder (Claude/Gemini) / Tracker (chainlink "bead-string" of verifiable issues) / Adversary (Sarcasmotron) / **Formal verification: Kani and Prusti** (e.g. proving nonce uniqueness for all inputs; fuzzing as substitute where provers don't apply) / "the security expert": **codescanner** — https://github.com/dollspace-gay/codescanner
- Case study: **Tesseract Vault** — one-shot script → production system, 977 unit tests, **FIPS 203/204 compliance** (ML-KEM/ML-DSA — the same standards family as Opake's hybrid wrapping)
- Thesis: hallucination-based termination gives a definitive "done" state vibe-coding can't reach; "by mid-2026... trust becomes the only premium product"

**"Introducing Chainlink"** (`3mkkrwjo5yk2e`) — intro-level; confirms breadcrumbs/session model, 500-line decomposition rule, universal context provider (Cursor/Aider/ChatGPT, not just Claude), `chainlink init --force` teaches the agent its own usage.

**"Beyond the Vibe"** (Sep 2025, `3ly6wwfxyys2c`) — the proto-VDD: hardened security-first prompts → automated audit → Generate→Audit→Refine recursive loop until the scanner finds nothing ("a dozen or more loops" for complex tasks). Shows the evolution: scanner loop (2025) → cross-model adversary + formal verification + tracker (2026).

### Negative findings
- No third-party experience reports (tools are recent + niche; crypto-oracle Chainlink saturates search) — confirmed negative. Patreon intro paywalled

### Sources
- https://github.com/dollspace-gay/chainlink · https://github.com/forecast-bio/crosslink · https://forecast.bio/crosslink/
- VDD gist: https://gist.github.com/dollspace-gay/45c95ebfb5a3a3bae84d8bebd662cc25 · VSDD gist: https://gist.github.com/dollspace-gay/d8d3bc3ecf4188df049d7a4726bb2a00
- http://dollspace.gay/ · https://leaflet.pub/p/did:plc:dzvxvsiy3maw4iarpvizsj67

---

## Strand 2: Spec-Driven Development Tooling (June 2026)

### The key taxonomy: three SDD maturity levels
- **Spec-First** — specs guide initial generation, then discarded. Drift accepted. (Kiro, Spec-Kit, BMAD)
- **Spec-Anchored** — specs persist and evolve alongside code; explicit back-edge. (OpenSpec, Spec Kitty)
- **Spec-as-Source** — only specs are edited; code regenerates. (Tessl)

The back-edge only genuinely exists in Spec-Anchored and Spec-as-Source.

### GitHub Spec-Kit
- `constitution` (immutable principles) → `/specify` → `/plan` → `/tasks` → implement. 5–8 files, 800–2000+ lines per medium feature
- Back-edge: weak (branch-per-spec; community built `spec-kit-sync` because drift detection is missing — a tell)
- Users: constitution pattern praised; otherwise "review overload", verbose markdown. Fowler's author preferred reviewing code over the markdown
- https://github.com/github/spec-kit · https://martinfowler.com/articles/exploring-gen-ai/sdd-3-tools.html

### AWS Kiro
- requirements.md (**EARS notation**: precondition/trigger/expected-response acceptance criteria) → design.md → tasks.md
- Back-edge: manual. Steering files (≈ CLAUDE.md) + Agent Hooks (event-driven spec-alignment checks)
- Users: EARS surfaces misunderstandings before code on complex features; but inflated a minor bugfix into 16 acceptance criteria
- Tool-specific (VS Code fork); EARS notation portable
- https://kiro.dev/docs/specs/ · https://petermcaree.com/posts/kiro-agentic-ide-hype-hope-and-hard-truths/

### OpenSpec (Fission-AI) — strongest back-edge, lightest ceremony
- 3-phase state machine: **propose → apply → archive**. `openspec/specs/` = current-state source of truth; `openspec/changes/` = active proposals (~250 lines vs spec-kit's 800). Delta markers ADDED/MODIFIED/REMOVED for brownfield
- **`/openspec:archive` consolidates what was actually implemented back into specs/** — the back-edge as a first-class command
- Won a Feb-2026 independent 13-category eval; repeatedly top pick for "small team + existing codebase". npm + TS, minutes to set up. Claude Code native via `/opsx:*`
- https://github.com/Fission-AI/OpenSpec · https://hashrocket.com/blog/posts/openspec-vs-spec-kit-choosing-the-right-ai-driven-development-workflow-for-your-team

### BMAD-METHOD
- 12–21 agent personas, two-phase planning→sharded stories. Enterprise/regulated multi-team governance. Highest ceremony of any tool — explicitly wrong for a 2-person team
- https://github.com/bmad-code-org/BMAD-METHOD

### Tessl (Guy Podjarny, $125M)
- Spec-as-source: one spec per code file, `tessl build` generates `DO NOT EDIT` code + Spec Registry (10k+ library specs against API hallucination)
- Fowler's author: MDD reborn — "inflexibility and non-determinism". Closed beta. Watch, don't adopt
- https://tessl.io/blog/tessl-launches-spec-driven-framework-and-registry/

### Superpowers (obra, Claude Code plugin)
- Most popular CC plugin. Flow: **Brainstorming (Socratic Q&A) → Options & Tradeoffs → Plan Sketch → Design Doc → Implementation Plan → Steps.** TDD + 4-phase debugging enforced
- The `brainstorming` skill: explore context → clarifying questions with pre-generated options → 2–3 approaches → design approval → design doc → plan. "Still my idea, my product, just done as a duo"
- No formal back-edge — planning discipline, not persistent-spec system
- Caveat: documented cases of skills silently not firing
- https://claude.com/plugins/superpowers · https://blog.codeminer42.com/brainstorming-the-skill-that-changed-claude-for-me/

### Honorable mentions
- **Spec Kitty** — spec-kit fork, git-worktree automation for parallel agents, Spec-Anchored. https://github.com/cameronsjo/spec-compare
- **GSD** — "SDD without the ceremony", minimal-artifact reaction to spec-kit bloat. https://pub.spillwave.com/what-is-gsd-spec-driven-development-without-the-ceremony-570216956a84

### Skeptical takes (load-bearing)
- **"Waterfall in Markdown"** (Alvis Ng): tested on a real project — 10x slower, more ceremony, same bugs. "If nobody read the Confluence page, nobody is reading your `.specify` folder." Specs omit tacit knowledge. https://medium.com/@iamalvisng/spec-driven-development-is-waterfall-in-markdown-e2921554a600
- **Fixed ceremony, poor downward scaling**: overhead is fixed, value scales with complexity → small tasks suffer most
- **Control illusion** (Fowler): agents still ignore/over-apply instructions despite specs
- **Discipline-dependent**: back-edge only works if "spec changes before code changes" is enforced; tooling doesn't supply discipline
- Critics concede SDD pays off on large complex systems — the federation-feature class, not the 2-hour-fix class

### Tooling shortlist
1. Superpowers `brainstorming` (or hand-rolled equivalent) — front edge, prose-before-code
2. OpenSpec propose→apply→archive — back edge, living spec doubles as investor-explanation substrate
3. EARS notation selectively, complex features only
4. Constitution/steering pattern — CLAUDE.md decisions #1–14 already are this
5. Spec-Anchored team norm: spec changes land before code; archive after. Enforceable via hook
- NOT: BMAD (ceremony), Tessl (immature, lock-in), full Spec-Kit (the documented ceremony collapse)

---

## Strand 3: Practitioner Experience Reports

### Armin Ronacher (36 posts in 2025)
- Claude Code, Sonnet, broad permissions, rarely interrupts. **Code style FOR agents**: simple descriptive functions over clever hierarchies, plain SQL over ORMs, permission checks locally visible, observability so a misbehaving agent is visible
- Friction: VCS/PR model inadequate — "I wish I could see the prompts that led to changes"
- Comprehension hedge: simple code stays reviewable
- https://lucumr.pocoo.org/2025/06/12/agentic-coding/ · https://lucumr.pocoo.org/2025/12/22/a-year-of-vibes/

### Mitchell Hashimoto (Ghostty) — closest role model for this team
- Flagship: +1641/−1125 PR, ~80% AI, **16 sessions, one commit per session**, $15.98 tokens, published in full
- **"I'm more or less the architect... I still like to come up with the code structure."** Writes function stubs + TODOs himself, agent fills them in — owns the skeleton by construction
- Oracle-plan-first; upfront design "prevented costly rewrites"; pre-planned fallbacks make pivots cheap
- Harness engineering: agent repeats a mistake → build test/lint + CLAUDE.md rule, not inline re-correction
- Parallel checkouts (ghostty2/3/4) racing same task, different models
- "Final manual review is super super super important"
- https://mitchellh.com/writing/non-trivial-vibing · https://zed.dev/blog/agentic-engineering-with-mitchell-hashimoto

### Harper Reed — canonical spec→plan→execute
- Idea honing ("ask me one question at a time") → `spec.md` → reasoning model → `prompt_plan.md` + `todo.md` → execute one prompt at a time, test between
- Planning doc as the checkpoint against getting "over my skis"
- https://harper.blog/2025/02/16/my-llm-codegen-workflow-atm/

### Geoffrey Huntley — Ralph Wiggum loop (extreme autonomy pole)
- `while :; do cat PROMPT.md | claude-code; done`; built a programming language in ~3 months
- Artifacts re-injected every loop: `@specs/*`, `@fix_plan.md`, `@AGENT.md`. **"Only one thing per loop"**
- Capture-the-why: agent documents why each test/impl matters "because future loops will not have the reasoning in their context window"
- **"No way in heck I'd use Ralph in an existing code base"** — greenfield-only by author's own admission
- A spec bug went undetected a month → spec quality is load-bearing
- https://ghuntley.com/ralph/

### Simon Willison — "Vibe engineering"
- Don't commit AI code you don't understand; "proudly and confidently accountable"
- 12-item prerequisite list: tests, planning, docs-for-agents, VCS habits, CI, review culture, agent management, manual QA, research, preview envs, when-not-to-use-AI, estimation
- "Normalization of Deviance" warning re auto-approve mode
- https://simonwillison.net/2025/Oct/7/vibe-engineering/

### Thorsten Ball / Sourcegraph (Amp)
- Division of labor: agent rough architecture → human moves guardrails → agent focused tasks. "Drawing the lines" not syntax crafting
- **Context pollution = critical failure mode**; fix via isolated-context subagents
- https://sourcegraph.com/blog/agentic-coding

### Steve Yegge & Gene Kim — CHOP
- "CHOP = Vibe Coding + Engineering. You leave your brain on." Counter to comprehension-abdication
- https://thedataexchange.media/vibe-coding-chop-steve-yegge/

### Anthropic official best practices
- Explore → Plan → Implement → Commit; skip planning only if "you could describe the diff in one sentence"
- **"Let Claude interview you" → SPEC.md → fresh session executes.** "Time spent making the spec precise pays off more than time spent watching the implementation"
- Verifiable checks so the human isn't the verification loop; adversarial review in fresh context (won't be biased toward code it just wrote); reviewer must flag only correctness/requirement gaps or it invents findings
- CLAUDE.md: per line ask "would removing this cause a mistake?"; bloat causes rule-dropping; ~300-line ceiling folklore
- Failure patterns: kitchen-sink session, correcting-over-and-over (→ /clear after 2 failures), trust-then-verify gap
- https://code.claude.com/docs/en/best-practices

### Birgitta Böckeler — "Harness engineering" (Apr 2026)
- Harness = everything except the model. **Guides (feedforward)** vs **Sensors (feedback)**; computational vs inferential controls
- Behaviour harness (functional correctness) = "the weakest area"
- "Whenever an issue happens multiple times, the feedforward and feedback controls should be improved"
- Agent has "no social accountability, no aesthetic disgust at a 300-line function, no organisational memory" — harness directs human input where it matters
- https://martinfowler.com/articles/harness-engineering.html

### Comprehension-debt literature (names pain (a) exactly)
- **Addy Osmani**: "the growing gap between how much code exists and how much any human genuinely understands." Causes: speed asymmetry + false-confidence signals (clean code, green tests). Warning signs: "it passed the tests" as approval criterion, architecture discussions citing what "the AI decided". Tests alone fail; specs alone fail ("translating a spec involves an enormous number of implicit decisions"); only fix = ruthless explicitness up front + maintained system-level mental model. "Making code cheap to generate doesn't make understanding cheap to skip." https://addyosmani.com/blog/comprehension-debt/
- **Jo Van Eyck — five retention techniques**: (1) design-first with why-this-over-alternatives; (2) comprehension as review gate — author must explain *why*, "the AI suggested it" is a logged flag; (3) active learning — explain edge cases rather than accept; (4) **system-level documentation walks** — trace a request through the full stack aloud, regularly; C4/dependency graphs to find comprehension hotspots; (5) measure understanding — time-to-root-cause, unassisted-debug success, onboarding depth. https://jvaneyck.wordpress.com/2026/03/21/comprehension-debt-the-hidden-tax-on-ai-generated-code/
- Academic grounding (not deep-read): arxiv 2604.13277, 2603.22106
- ADR-as-context: ADRs give the agent the why, stop re-litigating settled decisions; but miss implementation-level decisions → pair with capture-the-why in code at write time

### Cross-practitioner convergence
1. Spec/plan as first-class durable artifact, separate from code (universal)
2. Decompose into small individually reviewable/runnable units; comprehension preserved at chunk granularity
3. Harness over correction: repeated mistake → encoded rule
4. Adversarial fresh-context review
5. Human owns the shape/skeleton; agent owns fill-in
6. Capture the WHY at write time
7. Context hygiene as the dominant failure mode

### Divergences
- Greenfield vs brownfield autonomy (Ralph greenfield-only; Sourcegraph/Anthropic structured collaboration for brownfield — the fit here)
- Sonnet+token-thrift (Ronacher) vs rich-context-ignore-cost (Amp)
- Specs solve comprehension? Böckeler yes-materially vs Osmani necessary-but-insufficient (both hold: spec front-loads design, mental model catches residue)
- Tests as sufficient signal: gate correctness, not comprehension — track both

### Highest-value direct reads (strand 3)
- Hashimoto "Vibing a Non-Trivial Ghostty Feature" — https://mitchellh.com/writing/non-trivial-vibing
- Osmani "Comprehension Debt" — https://addyosmani.com/blog/comprehension-debt/
- Van Eyck "Comprehension Debt: The Hidden Tax" — https://jvaneyck.wordpress.com/2026/03/21/comprehension-debt-the-hidden-tax-on-ai-generated-code/
- Böckeler "Harness engineering" — https://martinfowler.com/articles/harness-engineering.html
- Willison "Vibe engineering" — https://simonwillison.net/2025/Oct/7/vibe-engineering/
- Anthropic best practices — https://code.claude.com/docs/en/best-practices
- Yegge beads corpus — steve-yegge.medium.com

---

## Strand 4: The Bun Zig→Rust AI rewrite (added 2026-06-09)

### What happened (confirmed via GitHub API + Sumner's PR body)
- **PR #30412 "Rewrite Bun in Rust"**, author Jarred-Sumner, merged **2026-05-14** into `oven-sh/bun` main. Branch `claude/phase-a-port`. **6,755 commits, 2,188 files, +1,009,257/−4,024 lines** (Zig removal came separately). Bun 1.3.14 = last Zig version; Rust port in canary, not yet stable as of 2026-06-09.
- **Full core rewrite, same architecture**: "The same architecture, the same data structures... No async rust" (Sumner). Passes the pre-existing test suite "on all platforms"; reported 99.8% pass on Linux x64 glibc; binary −3–8 MB; perf "between neutral and faster".
- Motives: memory safety (use-after-free/double-free as compile errors — "costed the team an enormous amount of development & debugging time"); **AI-legibility + Zig's ~Apr 2026 ban on LLM contributions** (Bun is Anthropic-owned since Dec 2025, ran an un-upstreamable fork). Sumner: "I expect OSS to go the opposite direction: no human contribution allowed."
- The reversal: 9 days pre-merge — "very high chance all this code gets thrown out completely." Passing tests changed the call.
- **First-party methodology blog post NOT yet shipped** — process detail below is reconstructed from committed artifacts + official audit + commentary.

### The spec artifacts (the part that matters)
**`docs/PORTING.md`** — 575 lines, retrieved verbatim (commit `46d3bc2`; deleted from main post-merge): https://github.com/oven-sh/bun/blob/46d3bc29f270fa881dd5730ef1549e88407701a5/docs/PORTING.md
- Written as a **prompt, second person**: "You are translating one Zig file to Rust. Read this whole document before writing any code."
- **Two-phase**: Phase A = faithful logic capture, one `.zig`→one `.rs`, need not compile; Phase B = make it compile crate-by-crate
- Ground rules: banned deps (tokio/rayon/hyper/futures/std::fs/net — "Bun owns its event loop and syscalls"); no `async fn`; mandatory `// SAFETY:` mirroring the Zig invariant; `// TODO(port):` for low confidence ("Flagging is better than wrong code"); `// PERF(port):` markers for downgraded idioms, Phase B greps + benchmarks; structural fidelity (same fn names, field order, control flow — "Phase B reviewers diff .zig ↔ .rs side-by-side")
- ~8 mapping sections: crate map, type map (~38 rows), idiom map (~70 rows: `defer`→Drop, `errdefer`→scopeguard, comptime→generics/macros), strings ("Data is bytes, not str"), allocators, pointers, collections, JSC GC-safety, FFI
- Every `.rs` ends with a `PORT STATUS` block (source, confidence high|medium|low, todo count, notes for Phase B)
- **Key insight: this is NOT a behavior spec — it's a per-construct TRANSLATION CONTRACT** that removes agent degrees of freedom on boring decisions and routes genuine ambiguity to TODO(port) or a lookup table

**`docs/LIFETIMES.tsv`** (confirmed by reference in PORTING.md) — pre-computed cross-file ownership classification for every raw-pointer struct field: cols `file·struct·field·zig_type·class·rust_type·evidence`; classes OWNED→Box, SHARED→Rc/Arc, BORROW_PARAM→&'a, ARENA→&'bump, UNKNOWN→Option<NonNull>+TODO. "Trust it over local guessing." **The load-bearing trick: the hardest whole-codebase decision (pointer ownership) was analyzed ONCE globally with evidence, then fed to per-file agents as a lookup.**

### Verification & orchestration
- Confirmed: test suite as the behavioral oracle; Claude Code agents; CI-error→fix loop (`claude/ci-auto-fix-NNNNN` branches); ~6 days, "multiple parallel Claude instances" on high-memory nodes (reported, consistent)
- **Unconfirmed** (michaellady gist reconstruction — branch deleted, PR squashed): multi-vote default-deny verification, claims ledger, ASM-level noalias hunt, audit docs (`ZIG_RUST_DIVERGENCE_AUDIT.md`, `NOALIAS_HUNT_REPORT.md`) deleted just before merge. Gist: https://gist.github.com/michaellady/7d552137fb1e37ab9bf637e450016c25
- **Differential testing / fuzzing: UNCONFIRMED** — appears only in secondary summaries; HN critique explicitly notes "no disclosed comparative testing, fuzzing, or CVE disclosure"

### Quality outcomes (Bun's own audit, bun.com/bun-unsafe-audit, 2026-05-21, itself "AI generated")
- **13,365 unsafe blocks** at audit (≈10.4k at merge — grew during cleanup). ~9,300 convertible to safe; ~4,000 must stay (FFI, perf)
- **"Five functions are unsound today"** — UB reachable from safe Rust
- The core critique: a *faithful* port of manual-memory-management Zig = manual memory management wearing Rust syntax. The safety dividend is a deferred post-merge campaign, not a property of the port
- Zero human approving review on a 1M-line merge; community split (bug-for-bug-then-refactor is standard practice vs. "mental models are crucial — nothing substitutes for having the program in your brain")

### Rewrite-tooling landscape (vs greenfield SDD)
- **Google (DIDACT)**: internal agentic migrations — prompt + few-shot + file:line targets; **80% of landed changes AI-authored, ~50% time reduction** (confirmed, research.google/blog/accelerating-code-migrations-with-ai/)
- **AWS Transform** (+Kiro): managed migration factory; "Custom Knowledge Items" = reusable migration patterns — conceptually Bun's spec as a product
- **Tessl**: `tessl document` reverse-engineers specs FROM existing code; spec-as-source maintenance; still beta, non-deterministic
- **Morph Fast Apply**: 7B apply-layer, ~10.5k tok/s — execution primitive for mass mechanical refactors, not a spec tool
- **Spec-Kit/Kiro/OpenSpec/BMAD**: greenfield/feature-scoped, poor fit for porting
- Generalizable pattern (Bun, Google, AWS): (1) author the pattern/contract once, (2) **pre-compute cross-cutting hard decisions globally**, (3) partition + parallel agents, (4) gate on tests + compile + auto-repair, (5) human review at the boundary (all except Bun)

### Lessons for this team
1. The spec that worked is a **translation contract, not a behavior spec** — third spec species alongside behavior specs (OpenSpec) and property specs (VSDD/Kani). CLAUDE.md decisions + the indexer "Conventions for agents" already ARE this species
2. **LIFETIMES.tsv move**: pre-compute the globally-hard decision once, hand agents a lookup with evidence. Direct map: the ts-rs/typeshare DTO-drift gap — a generated contract table both sides reference
3. Faithful-first/idiomatic-later is legitimate but the quality dividend is a **scheduled separate phase**, never a freebie
4. **Tests-as-oracle: append-only during migrations** — agents may add tests, never weaken; crypto-load-bearing paths get differential tests against the old implementation
5. Bun's one clear mistake a 2-person team can avoid: **no human approving review**. Keep spec + boundary contracts under human authorship
6. **Never delete the audit trail** (Bun deleted divergence/noalias audits pre-merge + squashed history — anti-pattern; FEDERATION.md-style permanence is right)

### Sources
Primary: https://github.com/oven-sh/bun/pull/30412 · PORTING.md (commit 46d3bc2, link above) · https://bun.com/bun-unsafe-audit · https://news.ycombinator.com/item?id=48132488 · https://news.ycombinator.com/item?id=48016880 · https://research.google/blog/accelerating-code-migrations-with-ai/
Reporting: theregister.com (2026/05/14 + 2026/05/05 pieces) · weeklyrust.substack.com "The Great Zig to Rust Experiment" · dasroot.net unsafe-block analysis · bytecode.news
