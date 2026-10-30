# 0008. Recorded time and transaction IDs

- **Status:** Accepted
- **Date:** 2026-10-29
- **Notion decision:** None

## Context

Saving a transaction (M2, SUM-9) is where the ledger first stamps two things: the time the books recorded it, and its ID. Both were left open on purpose (ADR 0003's open question, ADR 0006's "SUM-9 decides how IDs are generated").

The Ledger Core design doc (Failure 8) says recorded time must come from one clock, never from the machines running the code. The obvious "one clock" is the database's. The demo breaks that: effective times come from the simulated clock, which fast-forwards, while the database's clock stays on real time. A fast-forwarded transaction would then be recorded before it took effect, and `post` rejects an effective time later than the recorded time.

## Decision

**Recorded time comes from the injected `Clock` the store is given, not from the database. Transaction IDs come from a database sequence, fetched by the store before `post`.**

- **One timeline.** Effective and recorded time must be read from the same clock, so `effective <= recorded` holds in production (real clock), in the demo (simulated clock) and in tests (fixed clock).
- **No database default.** `recorded_at` has no `DEFAULT now()`; the store always supplies it.
- **Microsecond precision.** PostgreSQL `timestamptz` stores microseconds, so the store truncates the recorded time to microseconds. An effective time with finer precision is rejected (`TooPrecise`), so a retry reads back exactly what was sent and never looks like a conflict.
- **The clock never goes backwards.** Resetting the demo means starting a fresh database.
- **Log order does not come from recorded time.** M3 reads the log in `(db_tx_id, id)` order behind a safe line, because neither `db_tx_id` nor `id` follows commit order. Any stamp is taken before commit, so neither a database clock nor an app clock is exact; what must hold is `effective <= recorded` and a clock that doesn't go backwards.
- **At real scale** the production clock can itself be backed by the database's clock (asked for the time, then passed in). That keeps Failure 8's single clock source while the demo and tests swap in their own.
- **IDs are a `bigint` sequence** (`transaction_ids`). The store fetches one and passes it to `post`, so no clock is involved (a UUIDv7 would embed a timestamp). Gaps are fine. The same sequence serves as Failure 6's `sequence`.
- **Assumes a single primary database.** If writes are ever sharded (by tenant is the natural split), ID generation changes behind the opaque `TransactionId`.

## Alternatives considered

- **The database clock (`now()`):** rejected. It splits time in two in the demo and fails the `effective <= recorded` rule after any fast-forward.
- **UUIDv7:** rejected. It embeds a timestamp, so it needs a clock, and it is bigger than a `bigint` for no gain on a single database.
- **App-generated random UUIDs:** rejected. Nothing gives them an order, and they would still need a separate sequence for Failure 6.

## Consequences

- **ADR 0003's open question is answered** here, and so is ADR 0006's question of how IDs are generated.
- **Failure 8 in the Ledger Core design doc points to this ADR.**
- **Callers must pass effective times at microsecond precision,** or get `TooPrecise`.
- **The store takes a `Clock`,** which is the one place real time enters the ledger (`SystemClock`).
