---
name: ideating-features
description: This skill should be used when the owner wants open-ended product feature ideas for Summa — for example "give me feature ideas for X", "what should we build next", "brainstorm around the Policy context", or any request for net-new ideas rather than refining one already in hand. Reads docs/*.md live and checks Linear before pitching, to avoid duplicating existing or rejected work. Product-level only — no implementation detail. Hands off to refining-ideas when the owner picks an idea to develop.
---

## Purpose

Generate product-level feature ideas for Summa, grounded in the current state of `docs/*.md` and what's already tracked in Linear — never a remembered or hardcoded list of bounded contexts. This is the entry point of the `product-manager` agent's workflow; it produces conversation, not Linear writes.

## Process

1. Read `docs/*.md` in full, live — every bounded-context doc plus `docs/decisions.md`'s "Product & approach" section and any roadmap-ordering notes. Bounded contexts and their maturity (live vs. stub) change over time; never assume yesterday's list.
2. Query Linear (`mcp__linear__list_projects`, `mcp__linear__list_issues`) for existing Projects/Issues that already cover the territory being brainstormed, so ideas aren't pitched twice or re-litigated after rejection.
3. Generate candidate ideas at product level only: the problem or opportunity, why it fits the current roadmap ordering, which bounded context(s) it touches. No schema, code, or infra design — that's out of scope for this agent entirely.
4. Present the ideas conversationally. This step never writes to Linear — drafting a Project is `drafting-linear-projects`'s job, and only after `refining-ideas` has turned a pick into a ready spec.
5. If the owner picks one to develop further, hand off to `refining-ideas`.

## Additional resources

### Related skills

- **`refining-ideas`** — takes a picked idea and turns it into a structured, domain-reviewed spec.
- **`drafting-linear-projects`** — the only skill in this workflow that writes to Linear, and only from a finished spec.
