---
name: refining-ideas
description: This skill should be used when the owner wants an idea fleshed out or reviewed for fit — for example "help me flesh this out", "review this idea", "does this fit the platform", "is this a good idea", or when an idea has just been handed off from ideating-features. Checks the idea against docs/*.md's recorded constraints and invariants, auto-consults a *-domain-expert.md agent when one exists, and structures the result into a ready spec. Hands off to drafting-linear-projects once the owner confirms the spec is ready.
---

## Purpose

Turn a rough or picked feature idea into a structured product spec, checked against Summa's recorded constraints and reviewed by the relevant domain expert — before anything reaches Linear.

## Process

1. Read `docs/*.md` live and check the idea against recorded product constraints: roadmap ordering, the domain invariants in `.claude/CLAUDE.md`, the Scribe "AI proposes, deterministic core decides" principle, and anything the docs record as an open question the idea might touch. When the idea touches more than one of these constraints at once, reach for `sequentialthinking` to work through the interaction before concluding fit.
2. Structure the idea into: problem statement, why now, affected bounded context(s), fit against roadmap ordering, open questions/risks. Still no implementation detail — schema, code, and infra design belong to later pipeline stages.
3. Look for an agent file matching `*-domain-expert.md` in `.claude/agents/` and auto-consult it for a domain-fit review before calling the spec ready. If none exists yet, say so plainly and ask the owner whether to proceed without that review — never skip this step silently.
4. If the idea resolves a `docs/decisions.md` "Open questions" entry, or is otherwise a genuine new decision, invoke `recording-decisions` before considering the spec done.
5. Present the finished spec. This step never writes to Linear — hand off to `drafting-linear-projects` once the owner confirms the spec is ready.

## Additional resources

### Related skills

- **`ideating-features`** — the usual source of the idea this skill refines; also reachable directly when the owner brings their own idea.
- **`drafting-linear-projects`** — turns a finished spec into a Linear Project, only after explicit owner go-ahead.
- **`recording-decisions`** — invoked from step 4 whenever the idea resolves an open question or is a genuine new decision.
