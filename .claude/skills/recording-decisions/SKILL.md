---
name: recording-decisions
description: This skill should be used automatically whenever a genuinely new design or process decision gets made while working in the Summa repository — for example choosing a library or package, picking a data shape not already specified in the docs, resolving one of the "Open questions" in docs/decisions.md or docs/ledger.md, or settling a naming/architecture question that isn't already recorded. Not for decisions already logged, for routine implementation of something already specified, or for revisiting a recorded decision without the owner's explicit sign-off — that gets flagged to the owner, not silently logged. Appends a dated row to docs/decisions.md in the same change, per .claude/CLAUDE.md's working agreement item 7.
user-invocable: false
---

## Purpose

Keep `docs/decisions.md` an accurate, current log. `.claude/CLAUDE.md`'s working agreement states plainly: "a decision made while coding gets a row in `docs/decisions.md`." Nothing else in this repository's tooling enforces that rule — this skill is the enforcement. It is background discipline, not a user-facing workflow: apply it automatically whenever a genuine decision surfaces during a session, not only when explicitly invoked.

## Process

1. Recognize a decision when it happens. A genuine decision is a choice that wasn't already fixed by `docs/ledger.md`, `docs/architecture.md`, or an existing `docs/decisions.md` row — for example: picking a specific library or package (Falco vs. Giraffe, a specific NuGet package), choosing a concrete data shape or naming the docs left open, resolving one of the "Open questions" sections in `docs/decisions.md` or `docs/ledger.md`, or settling an architecture/process question that came up mid-session.

2. Distinguish from non-decisions. Implementing something already fully specified in the docs is not a new decision — do not log routine implementation. Following an existing recorded convention is not a new decision either.

3. Get the owner's confirmation of the choice before logging it. Decisions are agreed upon in conversation, not invented unilaterally — this mirrors the "explain before writing" spirit of the rest of the working agreement. If the decision would revise or supersede something already recorded, flag that explicitly and wait for the owner's sign-off rather than logging a silent override.

4. Append one new row to the appropriate table in `docs/decisions.md` (Product & approach / Stack / Ledger design, or a new table if the decision doesn't fit an existing one) in the same change as the code that depends on it. Use today's date in `YYYY-MM-DD` format, matching the existing rows.

5. Never edit or delete an existing row. `docs/decisions.md` is explicitly append-only — "A superseded decision gets a new dated row rather than an edit." A decision that supersedes an earlier one gets a new row that says so; the old row stays untouched.

6. Match the terse, one-to-two-sentence style of the existing rows. This is a log entry, not a design doc.
