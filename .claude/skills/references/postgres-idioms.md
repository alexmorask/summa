# PostgreSQL idiom reference

Condensed, cited reference for writing PostgreSQL (schema and query text) in this repository. Prefer a concrete rule over vague advice. This file lives at the `skills/` level, used by `writing-postgresql-code` (`../references/postgres-idioms.md`). Complements, doesn't duplicate, `fsharp-idioms.md`'s "I/O-layer F#" section — that file owns the F#-side calling convention (`task`/`use`/parameterization); this file owns the SQL/Postgres content itself, wherever it lives (a `.sql` file or a string literal inside an `NpgsqlCommand`).

## ⚠ Read this before writing Stage 6's `Append`

`docs/architecture.md` specifies: "appending the same idempotency key twice returns `Duplicate existingId`." A naive `ON CONFLICT (idempotency_key) DO NOTHING ... RETURNING id` does **not** produce that — on a conflict, `RETURNING` returns no rows at all, not the existing row ([PostgreSQL `INSERT` docs](https://www.postgresql.org/docs/current/sql-insert.html); confirmed independently at [yihangho.name](https://www.yihangho.name/posts/til-behavior-of-postgres-on-conflict-and-returning/)). Getting the existing id back in one round trip needs a no-op `DO UPDATE` instead: `ON CONFLICT (idempotency_key) DO UPDATE SET idempotency_key = EXCLUDED.idempotency_key RETURNING id` — this always returns a row, insert or hit ([QueryPlane](https://queryplane.com/blog/postgres-upsert/)). That clause alone can't tell you *which* happened (insert vs. conflict) — if the adapter needs to distinguish `Appended` from `Duplicate` (which `docs/architecture.md` requires), that distinction has to come from somewhere else — e.g. comparing the returned row's identity-column value against what was expected, or a second read. This is a Stage 6 design decision, not a review afterthought — raise it during planning, and it likely belongs in `docs/decisions.md` via `recording-decisions` once resolved.

## Naming conventions

Unquoted lowercase `snake_case` throughout — Postgres folds unquoted identifiers to lowercase, so snake_case never needs quoting ([PostgreSQL naming conventions](https://brainvoyage.blog/postgresql-naming-conventions-guide)). Primary keys: `{table}_id` or a bare `id`, picked once and held consistently. Foreign keys mirror the referenced table's singular form (`user_id` → `users`). Indexes: `idx_{table}_{column}`; unique constraints: `{table}_{field}_key`.

## `GENERATED ALWAYS AS IDENTITY`, not `SERIAL`

Already Summa's documented choice, correctly so: identity columns are SQL-standard, reject manual inserts into the generated column (SERIAL silently allows them, risking future collisions), and are resettable without knowing a backing sequence name ([Schneide Blog](https://schneide.blog/2025/09/08/using-generated-as-identity-instead-of-serial-in-postgresql/); [Bytebase](https://www.bytebase.com/reference/postgres/how-to/how-to-use-identity-column-postgres/)). What it does **not** guarantee: gap-free numbering — a rolled-back transaction still burns a sequence value. Fine for Summa's global-ordering use, since nothing depends on contiguity, only order.

## `jsonb`, not `json`

Default to `jsonb` — decomposed binary storage avoids reparse-on-every-read, and only `jsonb` supports indexing ([PostgreSQL docs §8.14](https://www.postgresql.org/docs/current/datatype-json.html)). If querying inside the payload, add a GIN index: default `jsonb_ops` supports any-path queries but reaches 60-80% of table size; `jsonb_path_ops` (only `@>`, `@?`, `@@`) is 20-30% of table size and the better fit for a stable, known payload shape like an event ([Crunchy Data](https://www.crunchydata.com/blog/indexing-jsonb-in-postgres)). Anti-pattern: don't put a column inside `jsonb` that gets filtered or joined on in a hot path — promote it to a real typed column instead.

## Constraints as correctness tools

`NOT NULL` and `CHECK` validate per-row, immediately, not deferred ([PostgreSQL docs §5.5](https://www.postgresql.org/docs/current/ddl-constraints.html)) — cheap, always-on defense in depth even though the F# smart constructor already enforces the same invariant; the two aren't redundant, they cover different failure modes (a bug in the F# layer, a future direct-SQL script, a different service writing to the same table). `CHECK` can't see other rows/tables — for cross-row invariants use `UNIQUE`, `EXCLUDE`, or `FOREIGN KEY`. For the append-only events table specifically: foreign keys are usually skippable — nothing should ever block an insert into the log. For the mutable `account_balances`/`projection_checkpoint` tables, ordinary FKs and `CHECK`s (e.g. a `direction` domain check) are appropriate and cheap.

## Indexing

B-tree is the default and right choice for equality/range/ordering on scalar columns. Composite index column order: equality-filtered columns first, then range-filtered, then `ORDER BY`-only columns — an index is only used efficiently via a left-to-right prefix match ([use-the-index-luke.com](https://use-the-index-luke.com/sql/where-clause/the-equals-operator/concatenated-keys)); "most selective column first" is a documented myth, not a rule ([use-the-index-luke.com](https://use-the-index-luke.com/sql/myth-directory/most-selective-first)). Partial indexes (`CREATE INDEX ... WHERE processed = false`) are the right tool for "index only the not-yet-processed rows" — directly relevant to the projection worker's polling query. Index for actual query patterns, not preemptively.

## Transactions and isolation

Default `READ COMMITTED` is adequate for ordinary reads/writes ([PostgreSQL docs §13.2](https://www.postgresql.org/docs/current/transaction-iso.html)); Postgres only truly implements two distinct levels (Read Committed and Serializable) — Read Uncommitted/Repeatable Read silently upgrade. The projection-write + checkpoint-advance atomicity requirement from `docs/architecture.md` needs no special isolation level — just the same `BEGIN...COMMIT` transaction wrapping both statements; Read Committed already guarantees a crash between them leaves neither applied. Reach for `SERIALIZABLE` only if a future rule needs anomaly-free concurrent correctness (requires app-level retry on serialization failure) — not needed for the current single-writer projection design.

## Query review habits

Use `EXPLAIN (ANALYZE, BUFFERS)`, not bare `EXPLAIN`, for real timings and actual index usage rather than planner guesses ([PostgreSQL docs §14.1](https://www.postgresql.org/docs/current/using-explain.html)); read plans bottom-up. Concrete smells: `SELECT *` defeats index-only scans and needlessly pulls large `jsonb` payload columns on hot paths; a `Seq Scan` with high "Rows Removed by Filter" signals a missing or unused index; high `loops` in a nested loop signals an N+1 pattern — directly relevant to a naively-implemented polling worker querying per-row instead of batching.

## Anti-patterns to flag on review

- `ON CONFLICT DO NOTHING ... RETURNING` relied on to produce the existing row's id — see the callout above; the one most likely to actually bite this project.
- `money` type or bare `float`/`real` for currency — use `BIGINT` minor units (Summa's existing rule). The `money` type is explicitly flagged by the [PostgreSQL wiki's "Don't Do This"](https://wiki.postgresql.org/wiki/Don%27t_Do_This) for locale-dependent display and fractional-cent mishandling.
- Plain `timestamp` instead of `timestamptz` — drops the UTC offset, silently conflates different real moments ([same wiki](https://wiki.postgresql.org/wiki/Don%27t_Do_This); [Bytebase](https://www.bytebase.com/reference/postgres/how-to/how-to-store-time-postgres/)).
- `char(n)` instead of `text` — pads with spaces, no performance benefit, surprising comparison semantics ([PostgreSQL wiki](https://wiki.postgresql.org/wiki/Don%27t_Do_This)).
- `SERIAL` instead of `GENERATED ALWAYS AS IDENTITY` in new tables.
- Missing `NOT NULL` on a column that's never legitimately absent.
- `jsonb` used for data that ends up filtered or joined on regularly instead of promoted to a real column.
- Missing index on a column driving a worker's polling `WHERE` clause.
