---
name: verifying-current-documentation
description: This skill should be used by any writing-* skill (currently writing-terraform, writing-dockerfiles, writing-github-actions-workflows) when its own static idioms reference file doesn't settle current provider/API/action syntax or behavior — for example a Terraform azurerm argument, an Azure resource's current behavior, a GitHub Actions marketplace action version, or Entra/OIDC specifics. Prefers Microsoft's own documentation (microsoft_docs_search, then microsoft_code_sample_search and microsoft_docs_fetch as needed) for anything Microsoft/Azure/Entra-specific as the authoritative source, falling back to WebSearch only for topics Microsoft doesn't own. Folds findings into the plan as explicitly external verification, distinct from the idioms file's static guidance, and never uses that verification to override an already-recorded docs/decisions.md decision on its own authority — only to surface a flagged concern. Not for accounting/domain-practice research — that's researching-domain-best-practices, a sibling skill for a different kind of question.
---

## Purpose

Give any `writing-*` skill facing an externally-evolving syntax surface — cloud provider arguments, resource API behavior, marketplace actions — a repeatable way to verify current facts against authoritative sources at plan time, rather than trusting a static idioms reference file that can quietly drift out of date. Same structural shape as `researching-domain-best-practices`, adapted for syntax/API verification instead of contested-practice research.

## Process

1. Recognize the trigger before reaching for this skill. Use it only when the plan needs a specific, concrete syntax/behavior answer that neither the calling skill's own idioms reference file nor this repo's docs already settle — not for every plan, and not a substitute for reading the idioms file first.
2. Frame a narrow question before searching. Name the specific argument, resource, action, or behavior in question, rather than a broad search.
3. Prefer Microsoft's own documentation for anything Microsoft/Azure/Entra/GitHub-Actions-runner-specific: `microsoft_docs_search` first for a quick, reliable overview, `microsoft_code_sample_search` for a working example, `microsoft_docs_fetch` for full depth on a specific page. Fall back to `WebSearch` only for topics Microsoft doesn't own — a third-party marketplace action's own changelog, for example.
4. Fold the finding into the plan under a distinct label, never blended silently into the idioms file's own static guidance. Phrase it as what the current documentation establishes, not as something the idioms file already said.
5. Never let a live finding override an already-recorded `docs/decisions.md` decision on its own authority — the same rule `researching-domain-best-practices` already holds for its calling agents. If a finding conflicts with a recorded decision, surface it as a flagged concern for the owner, not a recommended reversal.
6. If the finding reveals the idioms reference file itself is now stale, say so as a suggestion — "worth updating `../references/*-idioms.md`" — but don't edit that file mid-task; that's a separate, deliberate change for the owner to make.
7. Keep the output scoped to the plan at hand. This skill produces a verified answer for the current task, not a durable new reference file.

## Additional resources

### Related skills

- **`researching-domain-best-practices`** — the sibling pattern for a different kind of question: contested/ambiguous domain practice, used by `*-domain-expert.md` agents, rather than syntax/API verification used by `writing-*` skills.
- **`recording-decisions`** — invoked by the calling skill, not this one directly, if a verified finding resolves a genuine new decision.
