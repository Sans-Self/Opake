# Tasks: telemetry

This change is canon-only by design: it lands the constraints and the signal inventory ahead of any implementation. No collectors, pipelines, or dashboards ship here — those are future changes designed against this capability.

## 1. Canon

- [ ] 1.1 Noï red-pens the delta spec (five constraint requirements) and the signal inventory verdicts (collect / never-collect / undecided per entry)
- [ ] 1.2 Resolve or explicitly park the undecided entries (social-graph aggregates, operation latency, error context) — parking with a named unblock condition is a valid resolution

## 2. Gates

- [ ] 2.1 `just spec-lint` 0 dangling

## 3. Sync + archive

- [ ] 3.1 Sync delta to canon per design.md sync notes (capability + inventory location + open questions)
- [ ] 3.2 Archive the change
