# 0010. The ledger records each account's type and contra flag

- **Status:** Accepted
- **Date:** 2026-11-04
- **Notion decision:** None

## Context

Accounts are defined by the business, in code (ADR 0005). An entry stores only an account number. What that number means (asset or revenue, normal or contra) decides which side increases its balance, so changing it in a later release would silently change the meaning of every past entry. The database cannot refuse such a change, because it has never been told what an account is.

Reversals have a similar gap: a transaction should be reversed at most once, and checking that in code alone leaves room for two concurrent reversals.

## Decision

**The ledger keeps its own append-only record of the accounts it has posted to, and compares the chart against it on every save.**

- **A `recorded_accounts` table.** It holds the account number, type and contra flag, and no name: a name is only a label and may change freely.
- **Written on first use, inside the save.** The accounts a transaction touches that are not yet recorded are inserted in ascending number order, in the same database transaction as the entries, with `ON CONFLICT DO NOTHING`. Ascending order keeps two concurrent saves from deadlocking on each other's rows.
- **Compared in a separate statement.** The save then reads the rows back and checks type and contra against the chart. It is a separate statement because, under PostgreSQL's default Read Committed isolation, only a new statement sees a row that a concurrent first use committed while the insert waited. A mismatch fails the save with the permanent error `AccountChanged`.
- **Append-only, with a foreign key from entries.** The same triggers as every ledger table (ADR 0009) refuse edits, and `entries.account` references `recorded_accounts`, so a raw insert cannot post to an account the ledger has not recorded.
- **The freeze starts at the first entry.** An account can be defined, renamed or retyped in the chart until something is posted to it.
- **Each store caches the accounts it has confirmed,** filled only after the save commits, so a rolled-back save never marks an account as confirmed. Later saves skip the check for those accounts.
- **One reversal per transaction.** A unique constraint on the reversed-transaction column makes a second reversal fail with the permanent error `AlreadyReversed`, even when two reversals race.
- **Account identity is its number.** `Account` is not hashable, so a whole account cannot be a map or set key. Maps key by `AccountNumber`, as the chart does.

## Alternatives considered

- **Syncing the whole chart into the database at startup:** rejected. The ledger would own the account list. If only accounts that already have entries are frozen, a rolling deploy lets an old and a new version race the check.
- **Freezing from the moment an account is stored:** rejected. It is stricter than the ledger rules require and blocks fixing a mistake before anything is posted.
- **Copying the type onto every entry:** rejected. It would detect a change but not prevent it, the same weakness as in ADR 0005.
- **Checking in code only:** rejected, for the reasons in ADR 0009.
- **Share-locking the account rows on every save:** rejected. Every code path would have to remember to do it, and hot accounts would see constant lock traffic.

## Consequences

- **Raw entry inserts need a recorded account.** Tests and tools that insert entries directly must record the account first.
- **Renames are free,** because the name is not stored.
- **An optional read-only startup check is possible later,** comparing the chart with the recorded accounts to fail fast before the first save.
- **Accounts can later move to their own module above the ledger** without changing this table, since the ledger needs only the number, type and contra flag.
- **A permanent error needs a human.** `AccountChanged` and `AlreadyReversed` will not succeed on retry; the chart or the caller must change.
