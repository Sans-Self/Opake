## Agent Delegation: Issue Management

### Product Owner Agent

This project has a dedicated **product-owner** agent (`.claude/agents/product-owner.md`) responsible for all backlog management. Coding agents should delegate issue management work to it.

### What Coding Agents Do Themselves

These are the ONLY crosslink operations a coding agent should run directly:

- `crosslink quick "<title>" -p <priority> -l <label>` — to unblock yourself when the hook requires an active issue
- `crosslink session work <id>` — to mark what you're working on
- `crosslink comment <id> "<text>" --kind <kind>` — to document your own work (plan, decision, observation, blocker, resolution, result)
- `crosslink close <id>` — to close issues you completed
- `crosslink session end --notes "..."` — to end your session

### What Gets Delegated to the Product Owner

Spawn the `product-owner` agent (via the Agent tool with `subagent_type: "general-purpose"`) for:

- Creating epics with subissues
- Milestone assignment and creation
- Setting up dependency chains (`crosslink block`)
- Duplicate detection before creating new issues
- Backlog triage and prioritization
- Breaking down large features into trackable work
- Answering "what's left?" / "what should I work on next?"

### When You Discover New Work Mid-Implementation

If you find additional work while coding:

1. Create a bare issue: `crosslink create "<title>" -p medium --label feature`
2. Move on — the product owner will triage it into the right milestone, check for duplicates, and add context

Do NOT spend your context window on milestone lookups, dependency management, or backlog grooming. That's the product owner's job.
