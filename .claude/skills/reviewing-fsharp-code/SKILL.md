---
name: reviewing-fsharp-code
description: This skill should be used when the user asks to review F# code, review an F# diff, run code review on F# changes, check F# code for idiom or performance issues, or asks for a cleanup/simplification pass over F# — for example "review this F# change", "code review the F# diff", "check this stage's F# for idioms", or "simplify this F# code". Wraps the installed code-review and simplify skills with an added F#-idiom finder angle rather than re-implementing review from scratch.
---

## Purpose

Get F#-idiom-aware review coverage without duplicating the existing finder/verify pipeline. This skill does not run its own review process — it augments the already-installed `code-review` and `simplify` skills with one additional angle.

## Process

1. Confirm the reviewed diff actually touches `.fs` or `.fsi` files. If it doesn't, skip this skill's augmentation and just defer entirely to the plain `code-review`/`simplify` skill.

2. Check whether `reviewing-postgresql-code` also applies to the same diff (i.e. it also touches `.sql` files or embedded SQL command text — common for `Summa.Ledger.Store` changes). If so: invoke the Skill tool for `reviewing-postgresql-code` now, if its content isn't already loaded in this conversation, to get its angle-briefing — then carry both angles into one `code-review`/`simplify` invocation at step 4 instead of two. Do not invoke `reviewing-postgresql-code` more than once per review, and do not invoke the underlying `code-review`/`simplify` skill independently from each side — running that pipeline twice on one diff wastes work and risks two inconsistent reports.

3. Decide which underlying skill applies, same as normal:
   - Correctness bugs plus cleanup, at a given effort level → invoke `code-review:code-review`, passing through whatever target/effort/flags the user gave.
   - Cleanup only (reuse, simplification, efficiency, altitude — no bug hunting) → invoke `simplify`.

4. When that skill's Phase 1 dispatches its parallel finder angles (3+5 angles for `code-review`, 4 angles for `simplify`), add one more angle to the *same* parallel batch rather than running it as a separate pass: an **F# idiom angle**. Brief it to scan the diff's `.fs`/`.fsi` changes against `../references/fsharp-idioms.md` — specifically the "Anti-patterns to flag on review" list and the domain-modeling, error-handling, and performance sections — and return candidates in the same `file`/`line`/`summary`/`failure_scenario`-or-cost shape the other angles use, so they merge into the same dedup step.

5. Let the wrapped skill's own Phase 2 (verify, for `code-review`; apply, for `simplify`) proceed exactly as that skill defines it. The F# angle's candidates are just additional input to that pipeline — do not produce a second, separate report.

6. Also hold the Summa-specific invariants from `.claude/CLAUDE.md` in mind while judging F# candidates (pure domain core, `Result` over exceptions for domain errors, `int64` money, append-only ledger, `.fsproj` compile order) — these are project rules, not general F# style, and should be cited as such rather than attributed to the general idiom reference.

7. If a finding surfaces code that reflects a decision not already recorded in `docs/decisions.md` — a library choice, a data shape, a deviation from a documented convention — do not just report it as a normal finding. Flag it distinctly and invoke the `recording-decisions` skill (or point the owner at it) rather than letting an undocumented decision pass through review silently.

## Additional resources

### Reference files

- **`../references/fsharp-idioms.md`** — the F# idiom reference used for the added finder angle. Lives at the `skills/` level, shared with (not owned by) the `writing-fsharp-code` skill.

### Related skills

- **`reviewing-postgresql-code`** — coordinate per step 2 whenever a diff touches both `.fs` and SQL content, rather than each skill invoking the underlying pipeline separately.
- **`recording-decisions`** — invoked from step 7 whenever review surfaces an undocumented decision.
