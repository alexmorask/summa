---
name: reviewing-postgresql-code
description: This skill should be used when the user asks to review PostgreSQL/SQL changes, review a migration, check schema or query correctness, or asks for a cleanup pass over SQL in the Summa repository — for example "review this migration", "check this query for correctness", "review the Append changes", or "simplify this SQL". Applies to standalone `.sql` files and SQL query text embedded in F# (Npgsql command text) alike. Wraps the installed code-review and simplify skills with an added Postgres-idiom finder angle rather than re-implementing review from scratch.
---

## Purpose

Get Postgres-idiom-aware review coverage — schema and query correctness, not just F# structure — without duplicating the existing finder/verify pipeline.

## Process

1. Confirm the reviewed diff touches `.sql` files, or F# files containing embedded SQL command text (an `NpgsqlCommand`'s command text or similar). If neither, skip this skill's augmentation and defer entirely to the plain `code-review`/`simplify` skill.

2. Check whether `reviewing-fsharp-code` also applies to the same diff (i.e. it also touches `.fs`/`.fsi` files, which it usually will — the SQL mostly lives inside `Summa.Ledger.Store`). If so: invoke the Skill tool for `reviewing-fsharp-code` now, if its content isn't already loaded in this conversation, to get its angle-briefing — then carry both angles into one `code-review`/`simplify` invocation at step 4 instead of two. Do not invoke `reviewing-fsharp-code` more than once per review, and do not invoke the underlying `code-review`/`simplify` skill independently from each side — running that pipeline twice on one diff wastes work and risks two inconsistent reports.

3. Decide which underlying skill applies, same as `reviewing-fsharp-code`:
   - Correctness bugs plus cleanup, at a given effort level → invoke `code-review:code-review`, passing through whatever target/effort/flags the user gave.
   - Cleanup only → invoke `simplify`.

4. Add one more angle to that parallel batch: a **Postgres idiom angle**. Brief it against `../references/postgres-idioms.md` — especially the `ON CONFLICT ... RETURNING` correctness callout at the top (a real, easy-to-miss gap between the naive implementation and Summa's stated `Duplicate existingId` semantics), the constraints/indexing sections, and the anti-patterns list — and return candidates in the same `file`/`line`/`summary`/`failure_scenario`-or-cost shape the other angles use, so they merge into the same dedup step.

5. Let the wrapped skill's own Phase 2 (verify, for `code-review`; apply, for `simplify`) proceed exactly as that skill defines it. The Postgres angle's candidates are additional input to that pipeline, not a separate report.

6. Also hold the Summa-specific invariants in mind while judging candidates: money as `BIGINT` (never `float`/`money` type), the events table's append-only nature, idempotency enforced by the `UNIQUE` constraint (not re-implemented in application code), `timestamptz` for both `occurred_at` and `recorded_at`, `jsonb` for the payload, and the projection-write/checkpoint-advance atomicity requirement. These are project rules — cite them as such, not as general Postgres style.

7. If a finding surfaces code that reflects a decision not already recorded in `docs/decisions.md` — an indexing choice, a `jsonb` structure decision, how the `ON CONFLICT` gap got resolved — flag it distinctly and invoke `recording-decisions` (or point the owner at it) rather than letting an undocumented decision pass through review silently.

## Additional resources

### Reference files

- **`../references/postgres-idioms.md`** — the Postgres idiom reference used for the added finder angle, including the `ON CONFLICT`/`RETURNING` correctness callout. Lives at the `skills/` level, shared with (not owned by) `writing-postgresql-code`.

### Related skills

- **`reviewing-fsharp-code`** — coordinate per step 2 whenever a diff touches both `.fs` and SQL content, rather than each skill invoking the underlying pipeline separately.
- **`recording-decisions`** — invoked from step 7 whenever review surfaces an undocumented decision.
