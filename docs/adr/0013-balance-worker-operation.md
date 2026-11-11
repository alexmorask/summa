# 0013. Balance worker operation

- **Status:** Accepted
- **Date:** 2026-11-10
- **Notion decision:** None

## Context

ADR 0012 decides what a stored balance is. This ADR decides how the worker that maintains them runs. The hard parts are the ones the Ledger Core design doc names. A posting that finishes saving late must never be skipped (Failure 6). A poison entry must stop the worker, never be skipped (Failure 7). Two workers, a crash mid-batch and a rebuild must each leave balances correct with nothing counted twice.

ADR 0008 fixed the log order: `(db_tx_id, id)`, read only behind a safe line, because sequence numbers are handed out before commit and a later number can commit first.

## Decision

**One worker at a time reads the log behind the safe line in capped batches, moves a checkpoint in the same database transaction as the balances, stops on a permanent error, and polls rather than being pushed.**

- **The safe line.** The worker reads only transactions whose `db_tx_id` is below `pg_snapshot_xmin(pg_current_snapshot())`, after its checkpoint, ordered by `(db_tx_id, id)`. The checkpoint stores that pair. A save that was still open when the worker looked has a `db_tx_id` at or above the line, so it is read later, never skipped. A rolled-back save leaves a gap that never fills; the line moves past it without stalling.
- **Capped batches.** A batch is at most 1,000 ledger transactions (a setting), whole transactions only. A full batch means "go again straight away"; a short one means "wait".
- **One worker per checkpoint.** Each batch locks the checkpoint row when it starts and holds the lock to commit. A second worker waits, then continues from the moved checkpoint, so a standby is just a second worker.
- **Re-read after locking.** The worker reads the checkpoint after it has the lock, never before. A rebuild that runs while a worker waits is then safe: the worker continues from the reset checkpoint.
- **Rebuild is reset plus normal replay.** In one database transaction holding the same lock, delete all stored balances and reset the checkpoint to the start. The ordinary worker catches up. There is no second code path to disagree with normal processing. Readers during the replay see correct but old balances.
- **Errors.** Transient errors (a dropped connection, a deadlock) are retried a few times with a growing wait. A permanent error stops the worker and names the transaction it failed on. Nothing is ever skipped, including after N retries.
- **Netting in Rust.** Each batch is loaded through the existing row types, netted in a `HashMap` and added with `checked_add`. A total that would overflow is a permanent error.
- **Polling.** The loop runs a batch and either goes again or waits a short interval. Every wait goes through a `pause` closure the caller supplies, so the worker never reads the clock (ADR 0003) and tests never sleep.
- **No timeout on a stuck safe line.** A long-running transaction elsewhere only delays the worker. It shows up as lag, never as a wrong balance. This ticket adds no timeout or cap.

## Alternatives considered

- **Update balances in the same write as the posting:** rejected. Every posting to a busy account would contend for the same balance row, so one account serialises all its writers. The worker moves that contention to one place that can batch.
- **A dead-letter queue for failing entries:** rejected. Balances are cumulative, so skipping one entry makes every later balance on that account wrong, silently. Stopping is the only safe response.
- **Push notifications (such as `LISTEN`/`NOTIFY`) instead of polling:** rejected. A notification can be lost, so polling is needed anyway. A wake-up also does not mean the safe line allows reading yet.
- **A lock held for the worker's lifetime:** rejected. A crashed or stalled holder blocks a standby until its connection is noticed as dead. A per-batch row lock is released by the commit or the crash itself.
- **A conditional checkpoint update (compare and set) instead of a row lock:** rejected. Two workers would both do the batch's work and one would throw it away. The lock makes the second one wait instead.

## Consequences

- **The safe line is server-wide.** `pg_snapshot_xmin` covers every database on the PostgreSQL server, so a long transaction in any database on the same server delays the worker. The result is lag, never wrong balances. A dedicated ledger at real scale therefore means its own PostgreSQL server, not only its own database. The Ledger Core design doc says so.
- **Tests use a bounded `settle` wait.** The worker's tests run in parallel, each in its own database on one server, and they share the safe line. A test waits, bounded, for the line to pass its postings rather than assuming it already has.
- **A poison entry stops all balances behind it** until a person fixes the cause. That is deliberate, and a worker-stopped signal is needed to make it visible.
- **Lag is exposed as data** by `BalanceWorker::lag`, counted in ledger transactions rather than time. SUM-12 emits it, with the age of the oldest running database transaction and the worker-stopped signal.
- **Nothing runs the worker yet.** SUM-11 is library code. SUM-13 (the playground) is the first thing that runs it.
