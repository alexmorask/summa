---
name: updating-readme
description: This skill should be used at natural checkpoints in the Summa repository — the end of a build-plan stage, or right before a commit — to check whether README.md is still accurate given what changed, and propose targeted updates. Companion to updating-claude-md; both are auto-invoked from the same checkpoint, but this one owns README.md only. Not for docs/*.md (pre-code design/brainstorming docs) or docs/decisions.md specifically (owned by the recording-decisions skill). Typical triggers include finishing a build-plan stage, preparing to commit a stage's work, or being asked to "check if the docs are current" or "update the README".
---

## Purpose

Keep `README.md` accurate as the codebase evolves. This is checkpoint discipline, not a per-edit reflex — most individual changes invalidate nothing in it, and running a full assessment after every small edit would be noise, not signal. It fires at the end of a body of work (a stage, a commit), not mid-task.

Explicitly out of scope: `docs/*.md` (ledger.md, architecture.md, build-plan.md, glossary.md, and the context outlines) are pre-code design/brainstorming docs, not living operational docs — they don't get kept in sync with implementation the way README does. `docs/decisions.md` specifically is owned by the `recording-decisions` skill and has its own append-only update mechanism; this skill doesn't touch it. `.claude/CLAUDE.md` is `updating-claude-md`'s job, not this one's — the two stay separate skills, invoked together, because they check different failure modes on a file the owner cares about differently (README is project-facing documentation; CLAUDE.md is the repo's own working agreement).

## Process

1. Determine what actually changed since the last checkpoint — `git diff` against the last commit, or against whatever range makes sense for the checkpoint (a whole stage's uncommitted work, or the commits since the last time this skill ran). Don't guess at scope; check it.

2. Read `README.md` in full, current content.

3. Check specific, checkable claims against the actual change — not a vibe-based reread: the "Status" line, "Roadmap," "Architecture" table/prose, and "Documentation" table. Has an architectural claim actually changed, or only been implemented (implementing something already described isn't a discrepancy)? Has a `docs/*.md` file been added, removed, or repurposed such that README's own reference table/links to it are now wrong? (Fixing README's *link to* a docs file is in scope; editing the docs file's own content is not.)

4. Distinguish staleness from silence. A future stage not being implemented yet is not a discrepancy — README is allowed to describe target/future state. A claim that is now factually wrong (e.g. a stale "Status" line, or a missing command genuinely needed to build/run/test today) is.

5. For each real discrepancy, propose a minimal, targeted edit — never regenerate a section or file wholesale, and never touch unrelated prose while in there. State what's stale, why, and the specific replacement text, then wait for approval before writing — same discipline as `writing-fsharp-code`/`writing-postgresql-code`.

6. If nothing is actually stale, say so plainly and make no edits. Don't invent busywork changes to justify having run.

## Additional resources

### Related skills

- **`updating-claude-md`** — the companion skill for `.claude/CLAUDE.md`; both are invoked together at the same checkpoint but check different files for different reasons.
