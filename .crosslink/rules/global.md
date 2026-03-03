## Git Policy
- `git commit` requires an active crosslink issue
- `git push`, `git merge`, `git rebase`, destructive git commands are blocked — tell the user to do these manually
- Read-only git (status, diff, log, show, branch) is always allowed

## Code Quality
- Read files before editing. Complete features, don't stop partway.
- Verify unfamiliar APIs exist before using them (check docs, not guesses).
- For large implementations (500+ lines): epic with subissues, one at a time.
- Check auto-memory (`MEMORY.md`) before creating issues for new work.
