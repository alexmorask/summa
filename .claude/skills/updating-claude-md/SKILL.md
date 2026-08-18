---
name: updating-claude-md
description: This skill should be used at natural checkpoints in the Summa repository — the end of a build-plan stage, or right before a commit — to check whether .claude/CLAUDE.md is still accurate given what changed, and propose targeted updates. Companion to updating-readme; both are auto-invoked from the same checkpoint, but this one owns .claude/CLAUDE.md only. Not a substitute for the claude-md-management plugin's claude-md-improver skill — that's a separate, broader, rubric-based quality audit the owner runs manually/periodically; this skill is the narrow diff-driven correctness half, not a quality audit. Typical triggers include finishing a build-plan stage, preparing to commit a stage's work, or being asked to "check if the docs are current" or "update CLAUDE.md".
---

## Purpose

Keep `.claude/CLAUDE.md` accurate as the codebase evolves. This is checkpoint discipline, not a per-edit reflex — most individual changes invalidate nothing in it, and running a full assessment after every small edit would be noise, not signal. It fires at the end of a body of work (a stage, a commit), not mid-task.

This file is the repo's own working agreement, not just documentation about the repo — read by every session, and the thing the prime directive ("the owner must understand every line") applies to most directly. Treat proposed edits here with at least as much care as a code change, not less.

Explicitly out of scope: `docs/*.md` (ledger.md, architecture.md, build-plan.md, glossary.md, and the context outlines) are pre-code design/brainstorming docs, not living operational docs. `docs/decisions.md` is owned by the `recording-decisions` skill. `README.md` is `updating-readme`'s job, not this one's. Also out of scope: a broader structural/quality audit against a template — that's what the `claude-md-management` plugin's `claude-md-improver` skill does, run manually by the owner whenever they want one; this skill only checks whether specific, checkable claims are still true given what actually changed, not whether the file follows best practice generally.

## Process

1. Determine what actually changed since the last checkpoint — `git diff` against the last commit, or against whatever range makes sense for the checkpoint (a whole stage's uncommitted work, or the commits since the last time this skill ran). Don't guess at scope; check it.

2. Read `.claude/CLAUDE.md` in full, current content.

3. Check specific, checkable claims against the actual change — not a vibe-based reread. The "Status" section is the most likely place to actually need an edit; it says outright *"Update this section as the slice progresses, and add build/test commands here once they exist."* Does it still describe an earlier state than what now exists? Does it list the real build/test/run commands (e.g. `dotnet build`, `dotnet test`, `docker compose up -d`, `./db/migrate.sh`) if those now exist and it doesn't mention them?

4. Distinguish staleness from silence. A future stage not being implemented yet is not a discrepancy — CLAUDE.md is allowed to describe target/future state. A claim that is now factually wrong is.

5. For each real discrepancy, propose a minimal, targeted edit — never regenerate a section or file wholesale, and never touch unrelated prose while in there. State what's stale, why, and the specific replacement text, then wait for approval before writing — same discipline as `writing-fsharp`/`writing-sql`, with extra weight here since this is the working agreement itself.

6. If nothing is actually stale, say so plainly and make no edits. Don't invent busywork changes to justify having run.

## Additional resources

### Related skills

- **`updating-readme`** — the companion skill for `README.md`; both are invoked together at the same checkpoint but check different files for different reasons.
