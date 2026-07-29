---
name: updating-documentation
description: This skill should be used at natural checkpoints in the Summa repository — the end of a build-plan stage, or right before a commit — to check whether README.md and .claude/CLAUDE.md are still accurate given what changed, and propose targeted updates. Not for docs/*.md (pre-code design/brainstorming docs) or docs/decisions.md specifically (owned by the recording-decisions skill). Typical triggers include finishing a build-plan stage, preparing to commit a stage's work, or being asked to "check if the docs are current" or "update the README" / "update CLAUDE.md".
---

## Purpose

Keep `README.md` and `.claude/CLAUDE.md` accurate as the codebase evolves. This is checkpoint discipline, not a per-edit reflex — most individual changes invalidate neither file, and running a full assessment after every small edit would be noise, not signal. It fires at the end of a body of work (a stage, a commit), not mid-task.

Explicitly out of scope: `docs/*.md` (ledger.md, architecture.md, build-plan.md, glossary.md, and the context outlines) are pre-code design/brainstorming docs, not living operational docs — they don't get kept in sync with implementation the way README/CLAUDE.md do. `docs/decisions.md` specifically is owned by the `recording-decisions` skill and has its own append-only update mechanism; this skill doesn't touch it.

## Process

1. Determine what actually changed since the last checkpoint — `git diff` against the last commit, or against whatever range makes sense for the checkpoint (a whole stage's uncommitted work, or the commits since the last time this skill ran). Don't guess at scope; check it.

2. Read `README.md` and `.claude/CLAUDE.md` in full, current content.

3. Check specific, checkable claims in each against the actual change — not a vibe-based reread. Concretely:
   - **`.claude/CLAUDE.md`'s "Status" section** — this is the most likely place to actually need an edit; it says outright *"Update this section as the slice progresses, and add build/test commands here once they exist."* Does it still say "Pre-code" if code now exists? Does it list the real build/test/run commands (e.g. `dotnet build`, `dotnet test`, `docker compose up -d`, `./db/migrate.sh`) if those now exist and it doesn't mention them?
   - **`README.md`'s "Status" line, "Roadmap," "Architecture" table/prose, and "Documentation" table** — has an architectural claim actually changed, or only been implemented (implementing something already described isn't a discrepancy)? Has a `docs/*.md` file been added, removed, or repurposed such that README's own reference table/links to it are now wrong? (Fixing README's *link to* a docs file is in scope; editing the docs file's own content is not.)

4. Distinguish staleness from silence. A future stage not being implemented yet is not a discrepancy — README and CLAUDE.md are allowed to describe target/future state. A claim that is now factually wrong (e.g. "Status: Pre-code" after code exists, or a missing command genuinely needed to build/run/test today) is.

5. For each real discrepancy, propose a minimal, targeted edit — never regenerate a section or file wholesale, and never touch unrelated prose while in there. State what's stale, why, and the specific replacement text, then wait for approval before writing — same discipline as `writing-fsharp-code`/`writing-postgresql-code`. This applies with extra weight to `.claude/CLAUDE.md` specifically: it's the repo's own working agreement, not just documentation about the repo, so rewriting it deserves at least as much care as rewriting code.

6. If nothing is actually stale, say so plainly and make no edits. Don't invent busywork changes to justify having run.
