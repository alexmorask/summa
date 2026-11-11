# 0012. Stored balances

- **Status:** Accepted
- **Date:** 2026-11-10
- **Notion decision:** None

## Context

`balance()` adds up entries every time it is asked. That is correct and simple, but at real scale one busy account holds hundreds of millions of entries and a read takes seconds to minutes (Ledger Core design doc, Option 2). SUM-11 keeps balances ready ahead of time, so a read is one row.

That raises three questions. What exactly is stored for a balance? How does a stored balance relate to `balance()`, which has cutoffs for effective and recorded time? And what are these tables, given that ADR 0009 makes the ledger tables append-only in the database, while a worker must update balances and a rebuild must delete them?

## Decision

**A stored balance is two totals, debits and credits, both only growing, kept per account and per (account, tag kind, tag value). Stored balances are derived data, exempt from the append-only triggers, and "current" means no cutoffs.**

- **Two totals, not one.** Each row holds a debit total and a credit total in whole cents (`Money<Usd>`, added with `checked_add`). The net balance is worked out when it is read, on the account's normal side. The worker therefore never needs the chart or an account's type, and neither total can go negative.
- **Tag balances for every tag.** One stored balance per (account, tag kind, tag value), with the same two totals, kept for every kind found on entries rather than a configured list. They key on the stored `kind` and `value` text, so the ledger needs no business words (ADR 0005, D6). An untagged entry adds to no tag balance, so tag balances need not sum to the account total.
- **Whole ledger transactions per batch.** A batch never splits a transaction, so the committed totals always balance. Only whole-account balances go into a trial balance check, because tag balances need not sum.
- **Derived data, not ledger records.** The append-only triggers (ADR 0009) cover `transactions`, `entries` and `entry_tags` only. The balance and checkpoint tables get none, because the worker updates them and a rebuild deletes them. The entries stay the only source of truth.
- **"Current" means no cutoffs.** A stored balance counts every posted transaction, including one with a future effective time, and equals `balance()` with `Cutoffs::NONE`. As-of questions still sum entries directly and never read stored balances.

## Alternatives considered

- **One signed net total per balance:** rejected. Adding an entry to a net needs to know whether the account's normal side is debit or credit, so the writer would have to load the account's type. The total can also go negative, which breaks the rule that stored totals are never negative.
- **Debits minus credits:** rejected for the same reasons. It is a signed total, and the sign convention has to be applied again on every read.
- **Triggers on the balance tables, like the ledger tables:** rejected. Updates and rebuild deletes are the point of derived data. Protecting them would make rebuild impossible.
- **Stored balances with cutoffs or as-of snapshots:** deferred. They need a snapshot design of their own. As-of questions keep summing entries.

## Consequences

- **Balances can be deleted and rebuilt** from entries at any time, with no loss. A rebuild must match the incrementally maintained balances exactly.
- **`balance()` stays the definition.** A stored balance is correct when it equals `balance()` with no cutoffs for the same entries, and tests check that.
- **Stored balances lag the log,** so they can never gate an action such as a spend check. Those must read entries or use a different mechanism.
- **A tag balance is not a subtotal of the account balance,** and nothing may assume it is.
- **Reads need the chart** for the normal side, which the store already holds. A known account with no stored row reads as zero; an account missing from the chart is an error, as in `balance()`.
