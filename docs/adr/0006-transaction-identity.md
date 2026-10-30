# 0006. Transaction identity

- **Status:** Accepted
- **Date:** 2026-10-23
- **Notion decision:** None

## Context

A mistake in the ledger is corrected with a reversing transaction linked to the original (the append-only rule). Until now a transaction had no identity, so nothing could be linked to it.

The link has to go in one direction, and it has to respect that ledger records never change.

## Decision

**A posted transaction has an opaque `TransactionId`, and a reversal stores the ID of the transaction it reverses.**

- **The ledger stamps the ID at posting.** `post` takes the ID as an argument, the same way it takes the recorded time (ADR 0003). Pure code never makes one up, and an unposted transaction has none.
- **`TransactionId` is opaque.** It can be compared, ordered, hashed and copied. It wraps a `u64` for now (see Consequences).
- **The link is the reversal's `reverses` field.** The builder sets it with `reversing(original)`, and posting keeps it. It lives on the reversal because the original can never be edited to point forward.
- **`build` doesn't check the link.** The builder can't see the original. What can be checked without storage is checked when the reversal is made: `PostedTransaction::reversal` and `UnpostedTransaction::check_reversal_of` confirm the reversal mirrors the original and links to it. Whether the original exists and whether it was already reversed are M2's job: storage loads the original by ID, and a unique constraint on `reverses` enforces "reversed at most once".

## Alternatives considered

- **Link by idempotency key:** rejected. The key is a natural key, but it is long, caller-controlled text, and it doesn't exist yet. An ID the ledger assigns is short, can't collide, and is under the ledger's control.
- **No link until M2:** rejected. The Ledger Core Spec requires a reversal to be linked to its original, so a reversal without a link doesn't meet the rule.

## Consequences

- **`post` takes an ID.** Callers of `post` (ledger storage, from M2) supply it.
- **`TransactionId` wraps a `u64` until SUM-9.** SUM-9 decides how IDs are generated, together with the open question in ADR 0003 about where recorded time comes from; both are stamped at posting by the same part of the ledger.
- **Finding out whether a transaction T is reversed means searching** for a transaction whose `reverses` is T. The original carries no marker.
- **Answered by [ADR 0008](0008-recorded-time-and-transaction-ids.md):** IDs come from a database sequence.
