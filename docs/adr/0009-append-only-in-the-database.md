# 0009. Append-only enforced in the database; forward-only migrations

- **Status:** Accepted
- **Date:** 2026-10-29
- **Notion decision:** None

## Context

Ledger records are never edited or deleted. Until M2 that was a property of the Rust types, which only holds for code that goes through them. Once records live in PostgreSQL, anything with database access (a hand-written query, a future tool, a mistake) can change history. The balance rule has the same gap (Ledger Core design doc, Failure 3).

## Decision

**The database refuses edits and unbalanced transactions itself, and migrations only move forward.**

- **Append-only triggers.** Row-level triggers reject `UPDATE` and `DELETE`, and statement-level triggers reject `TRUNCATE`, on every ledger table, for every role.
- **Balance backstop.** A deferred constraint trigger checks at commit that each transaction has at least two entries and equal debits and credits. The rule lives in code and in the database on purpose: code gives the friendly error; the database stops hand-written inserts.
- **Forward-only migrations in production.** There are no down migrations. From Go live (D14) an applied migration file is never edited, and columns and tables that hold ledger history are never dropped (deprecate instead). Before then the dev database holds nothing worth keeping, so applied migrations may be edited and the database torn down and rebuilt at any time. Because the triggers forbid `UPDATE`, once data exists there are no backfills: new facts go in new nullable columns, defaults or new tables.
- **DDL is not covered.** Triggers don't fire on `DROP COLUMN` or `DISABLE TRIGGER`, and the table owner or a superuser can still bypass them. The drop policy lives in `CLAUDE.md` and review. Separate application and owner roles, with no edit permissions for the application, are the real-scale addition.
- **Storage lives in `crates/ledger`, behind the `postgres` feature.** Rust has no friend crates, and `post` plus the constructor that rebuilds a transaction from storage must stay crate-private, so the store has to sit in the same crate.
- **Queries are checked at compile time** (`sqlx` macros), with the query descriptions committed in `.sqlx/` so builds need no database.
- **Each test gets a fresh database** (`#[sqlx::test]`), because deleting rows between tests is impossible.

## Alternatives considered

- **App-only enforcement:** rejected. It protects nothing against code or people that bypass the app.
- **Role permissions only (no `UPDATE`/`DELETE` grants):** rejected as the sole mechanism. It depends on role setup being right everywhere, and the owner role bypasses it. Worth adding at real scale, on top of the triggers.
- **A separate `ledger-store` crate with a lint banning `post`:** rejected. It would make `post` public, so the compiler stops guarding it.
- **Runtime-checked queries:** rejected. Mistakes in SQL would surface at run time instead of at build time.
- **A shared test database, or rolling back after each test:** rejected. A shared database leaks state between tests, and a rollback can't test what happens at commit (the balance check).

## Consequences

- **Corrections are reversing transactions,** as before; the database now agrees.
- **Schema changes are additive in production.** From Go live a mistake in a migration is fixed by a new migration; before then, edit it and rebuild the dev database.
- **Tests and `cargo test` need the Docker database running;** `cargo build` and `cargo clippy` do not.
- **Changing a query or migration means re-running `cargo sqlx prepare`** and committing `.sqlx/`.
