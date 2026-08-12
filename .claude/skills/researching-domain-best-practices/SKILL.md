---
name: researching-domain-best-practices
description: This skill should be used by any *-domain-expert.md agent when a spec under review raises something its own baked-in domain knowledge and this repo's docs don't clearly settle — for example a scenario that's ambiguous, contested, or evolving in the domain's general practice. Searches toward authoritative sources (standards bodies, official specifications, primary references) over blogs, forums, or unattributed listicles, folds findings into the review as explicitly external research distinct from repo-derived checks, and never uses that research to override an already-recorded decision on its own authority — only to surface a flagged concern for the owner. Domain-agnostic and shared across every current or future domain-expert agent, not specific to any one domain.
---

## Purpose

Give any `*-domain-expert.md` agent a repeatable process for researching external domain best practices at review time — recognizing when its own static/baked-in knowledge doesn't settle something a spec raises, searching toward authoritative sources rather than low-quality secondary ones, and folding findings into a review clearly tagged as external research, never silently blended with the agent's repo-derived checks. Shared across domain experts rather than owned by any one of them, since this pipeline's own recorded decision already allows for more than one `*-domain-expert.md` agent to exist.

## Process

1. Recognize the trigger before reaching for this skill. Use it only when the spec-in-progress raises a question that neither the calling agent's own static/baked-in domain-knowledge section nor this repo's docs (which the calling agent should already have re-read) clearly settle — not for every review, and not as a substitute for re-reading the repo's own docs first.
2. Frame a narrow question before searching. Name the specific ambiguity the spec raises rather than running a broad "domain best practices" search, and avoid re-researching ground the calling agent's static knowledge already covers.
3. Search toward authoritative sources — standards bodies and official specifications, primary/original publications, and recognized professional or regulatory references. Treat marketing blogs, unattributed listicles, forum answers, and other low-quality secondary content as, at best, a pointer toward a better source — never cite one as the finding itself.
4. Cross-check a finding against at least two independent authoritative sources before treating it as settled. If sources conflict, or the practice is genuinely unsettled or contested, say so plainly rather than picking one silently.
5. Fold findings into the review under a distinctly labeled tag or section, never blended into repo-derived findings. Phrase repo-derived findings as what this repo's own docs establish; phrase external findings as what the researched practice or standard establishes, and how it relates to the spec at hand.
6. Never let external research override an already-recorded decision on its own authority — the same rule the calling agent already holds for its baked-in checklist. If a finding conflicts with an already-recorded decision, surface it as a flagged concern for the owner, not a recommended reversal.
7. Keep the output scoped to the review at hand. This skill produces findings for the current spec, not a durable new reference file. If the same question is likely to recur, say so as a suggestion (e.g. "worth adding to the static domain-knowledge section"), but don't edit the calling agent's own file mid-review — that's a separate, deliberate change for the owner to make.

## Additional resources

### Related skills

- **`recording-decisions`** — invoked by the calling pipeline stage (not by this skill directly) if a research finding leads the owner to settle a genuine new decision.
