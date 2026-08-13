---
name: assessing-feasibility
description: This skill should be used by tech-lead when a Linear Project (from product-manager, or self-originated for infra-only work) needs a technical feasibility review before it's broken into Issues — for example "is this buildable", "review this Project for feasibility", or automatically as the first step whenever a Project reaches tech-lead. Re-reads docs/architecture.md, docs/decisions.md, and .claude/CLAUDE.md's domain invariants live, auto-consults a *-domain-expert.md agent when the Project touches its bounded context, and returns a structured feasibility verdict. Hands off to planning-issues once the Project is feasible.
---

## Purpose

Determine whether a Linear Project is technically buildable given Summa's current architecture, recorded decisions, and domain invariants — before any Issue gets cut for it.

## Process

1. Read the Project's current spec from Linear (`mcp__linear__get_project`).
2. Re-read `docs/architecture.md`, `docs/decisions.md`, and `.claude/CLAUDE.md`'s domain invariants live — never from memory; these change as the project grows.
3. Look for an agent file matching `*-domain-expert.md` in `.claude/agents/` and auto-consult it whenever the Project touches its bounded context — the same glob-discovery pattern `refining-ideas` already uses. If none applies, say so plainly rather than skipping the step silently.
4. Assess technical feasibility: architecture fit, infra readiness (does the needed infra already exist, or is this blocked on infra work first), and ordering dependencies against other in-flight or planned work.
5. Return a structured verdict — Feasible / Feasible with concerns / Not feasible yet / Needs owner decision — with findings grouped by source (architecture fit, domain-expert review, dependency ordering). If a finding resolves a `docs/decisions.md` open question or is otherwise a genuine new decision, invoke `recording-decisions` before considering the review done.
6. Hand off to `planning-issues` once the Project is feasible (or feasible with concerns the owner accepts).

## Additional resources

### Related skills

- **`planning-issues`** — takes the feasible Project this skill signs off on and breaks it into Issues.
- **`recording-decisions`** — invoked from step 5 whenever the review resolves an open question or is a genuine new decision.
