---
name: writing-sql
description: This skill should be used when the user asks to write or draft PostgreSQL schema or query code in the Summa repository — for example "write the events table migration", "add the account_balances migration", "write the Append query", "add the checkpoint-advance transaction", or any request producing new or modified `.sql` files or SQL query text embedded in F# (Npgsql command text). Covers drafting a plan, writing idiomatic Postgres, and explaining unfamiliar constructs afterward, per this repo's working agreement. Applies to both standalone migration files and SQL authored inside `Summa.Ledger.Store`.
---

## Purpose

Write correct, idiomatic PostgreSQL for the Summa event store and its projections — schema (migrations) and query text alike — while honoring `.claude/CLAUDE.md`'s working agreement: the owner must understand every line, so this is a plan-then-write, approval-gated process, not a one-shot action.

Scope note: this skill covers the SQL *content* — schema design, constraints, indexing, query correctness — wherever it lives, whether that's a standalone `.sql` migration file or a string literal inside an `NpgsqlCommand` in `Summa.Ledger.Store`. When the work also involves F# structure around that SQL (the `task`/`use`/parameterization mechanics of calling it), `writing-fsharp` applies at the same time — draft both together rather than treating them as sequential, separate tasks.

## Process

1. Read `.claude/CLAUDE.md` in full, and the relevant sections of `docs/architecture.md` (event store & database decisions, write path) and `docs/ledger.md` (chart of accounts, money representation) for the task at hand. Scope is whatever the owner asked for directly, or whatever a Linear Issue handed off via `implementing-linear-issues` specifies — don't expand beyond it.

2. Draft an implementation plan before writing anything — file by file (or query by query), what changes and why. Use Plan Mode (`EnterPlanMode` / `ExitPlanMode`) for explicit approval before anything is written, same discipline as `writing-fsharp`.

3. Before finalizing the plan, read `../references/postgres-idioms.md` in full — in particular its callout on `ON CONFLICT ... RETURNING` semantics if the task touches the event store's `Append` path. That callout describes a real gap between the naive implementation and what `docs/architecture.md` actually requires (`Duplicate existingId` on a conflict); resolve it during planning, not after writing code that doesn't do what the docs say it does.

4. Hold these Summa-specific invariants as non-negotiable, overriding generic Postgres advice when they conflict:
   - Money is `BIGINT` minor units — never `float`, `real`, or the Postgres `money` type.
   - The events table is append-only — schema and queries alike must never support `UPDATE`/`DELETE` on posted rows.
   - Idempotency is enforced by a `UNIQUE` constraint on `idempotency_key` — this is the mechanism, not a belt-and-suspenders addition to an application-level check.
   - `occurred_at` (business time) and `recorded_at` (system time) are separate columns; both `timestamptz`, never bare `timestamp`.
   - The event payload is `jsonb`.
   - The projection write and checkpoint advance happen in the same transaction — never as two independently-committed statements.

5. Migration file organization is decided (see `docs/decisions.md`, Stack table, 2026-07-29): plain, sequentially-numbered `.sql` files (e.g. `0001_events_table.sql`) in `db/migrations/`, applied manually via psql for now — no DbUp/FluentMigrator/Evolve. **Forward-only**: never write a down script or roll back in place. A correction is a *new* migration that reverses or fixes a previous one — the same append-only, reversing-entry shape as the ledger's own domain rule, not a separate rollback mechanism. Number new migrations sequentially after the highest existing prefix in `db/migrations/`.

6. After the plan is approved, write the SQL (and, where applicable, coordinate with `writing-fsharp` for the surrounding F#).

7. After writing, explain every non-obvious Postgres construct used, in the chat reply — never in code comments. This is mandatory, not optional polish, same as the F# skill's equivalent step.

8. If planning or writing surfaced a genuinely new decision not already covered by step 4/5's resolved invariants — an indexing strategy, a `jsonb` structure choice, how the `ON CONFLICT` gap gets resolved — invoke `recording-decisions` before considering the task done.

## Additional resources

### Reference files

- **`../references/postgres-idioms.md`** — naming conventions, identity columns, `jsonb`, the `ON CONFLICT`/`RETURNING` correctness callout, constraints, indexing, transactions/isolation, query review habits, and Postgres anti-patterns.

### Related skills

- **`writing-fsharp`** — applies simultaneously whenever the SQL is called from `Summa.Ledger.Store`; owns the F#-side calling convention (`task`, `use`, parameterization) this skill doesn't duplicate.
- **`recording-decisions`** — invoked from step 8 whenever a new decision surfaces (the migration-tooling call itself is already resolved, per step 5).
