---
name: reviewing-engineer-code
description: This skill should be used by tech-lead when an engineer has opened a PR for a Linear Issue and it needs review — for example "review this PR", "check this diff", or automatically whenever a software-engineer/infrastructure-engineer PR is ready. Wraps the feature-dev:code-reviewer subagent for CLAUDE.md-aware bug/quality findings rather than reinventing review criteria, auto-consults a *-domain-expert.md agent when the diff touches its bounded context, and always returns a drafted review for the owner — never posts a gh pr comment or gh pr review itself.
---

## Purpose

Review an engineer's PR by reusing this repo's existing review tooling, and hand the owner a draft — never post feedback to GitHub directly.

## Process

1. Identify the PR/diff in scope using the read-only `gh pr view`/`gh pr diff` access already granted to `tech-lead` — never `gh pr comment` or `gh pr review`, which this skill must never call.
2. Delegate to the `feature-dev:code-reviewer` subagent via the `Agent` tool, passing the diff/scope to review. This reuses its existing CLAUDE.md-aware bug and quality scan with 0–100 confidence scoring rather than reinventing review criteria — deliberately not the `/code-review` slash command, which posts a `gh pr comment` directly and has no return value to intercept.
3. If the diff touches a bounded context with a `*-domain-expert.md` agent (e.g. Ledger/Recognition posting logic), also auto-consult it for a domain-correctness pass the generic reviewer wouldn't catch.
4. Fold both into one structured draft review, grouped by severity (Critical/Important), with file:line and a concrete fix suggestion for each finding — explicitly labeled as a draft, not a posted review. When a finding's severity or correctness is non-obvious — e.g. conflicting signals between the two reviews, or a subtle control-flow issue — reach for `sequentialthinking` to trace it explicitly before assigning severity. Before accepting any finding as Critical/Important, steel-man it: state the strongest faithful case that it's wrong or overstated, name one concrete observation that would overturn that case, then keep, downgrade, or drop the finding — naming the countercase without deciding isn't enough.
5. Present the draft to the owner. Posting it — via `gh pr comment`, `gh pr review`, or otherwise — stays the owner's call, never this skill's.

## Additional resources

### Related skills

- None. This skill's review criteria come from the `feature-dev:code-reviewer` subagent and, when relevant, a `*-domain-expert.md` agent — not from a sibling skill in this pipeline.
