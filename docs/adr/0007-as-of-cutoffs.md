# 0007. As-of cutoffs

- **Status:** Accepted
- **Date:** 2026-10-23
- **Notion decision:** None

## Context

The Ledger Core Spec asks for balances "as of" either of a transaction's two times (effective and recorded). It doesn't say whether a transaction at exactly time T is included, or whether both times can be used at once.

## Decision

**A cutoff means strictly before T. A query takes an effective cutoff, a recorded cutoff, both, or neither.**

- A transaction counts if it passes every cutoff that is set. With none set, everything counts.
- Both cutoffs together answer "what did the report on period P say when it was run at T": effective before the end of P, recorded before T.

## Why

- **Back-to-back periods fit exactly.** A period's end is the next period's start. A transaction at that instant belongs to the later period only, so periods never overlap or leave a gap, at any precision (Rust keeps nanoseconds, PostgreSQL microseconds). Month close (M9) relies on this.
- **One cutoff can't answer the combined question.** Only a query with both cutoffs can reproduce an earlier report.

## Alternatives considered

- **Inclusive, "up to and including T":** rejected. The last instant of a period depends on the precision, so a period's end has to be written as a different value in each system.
- **Two separate queries, one per time:** rejected. It is no less code, and it loses the combined question.

## Consequences

- **APIs say "before", not "as of".**
- **A transaction at exactly T is excluded from "before T".** To include one, the cutoff has to be later than its time.
