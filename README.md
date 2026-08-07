# Summa

An event-sourced, double-entry billing ledger written in F#. Immutable transaction
log, projected balances, and a provable trial balance — deployed on Azure with
Terraform.

> **Summa** — in F#, discriminated unions are *sum types*; in accounting, Luca
> Pacioli's *Summa de Arithmetica* (1494) first codified double-entry
> bookkeeping. It also just means *the total*.

**Status:** the ledger vertical slice — write/read API, projected balances, and
revenue recognition — is built and deployed on Azure. See
[`docs/decisions.md`](docs/decisions.md) for what's been decided and why.

## What this is

A billing platform built ledger-first: usage metering → rating → invoicing →
settlement → reconciliation. It is deliberately *not* a clone of the familiar
Customer/Subscription/Plan/Invoice resource model.

Three ideas define it:

**The ledger is the only source of truth.** Every money fact is an entry in a
single, globally-ordered, append-only double-entry log. Nothing is ever edited or
deleted; corrections are reversing entries. Because each transaction balances
locally, the global invariant — every account sums to zero — holds automatically
and is provable by replay at any time.

**Everything else is computed.** Balances, statements, and invoices are
projections folded from the log, not mutable records that get updated. An invoice
is a pure function of usage and pricing over a period, so regenerating a past
invoice is just calling the function again. Re-rating history after a pricing
change is a replay, not a migration.

**Pricing and revenue recognition are separate axes.** A *Policy* answers *how
much* (a composable pricing algebra, not a fixed plan bundle). A *Performance
Obligation* answers *how that amount becomes earned over time*. The same $120
can be a one-time fee or twelve months of service — same price, different
recognition.

## Architecture

```
Command → decide (pure) → Event → append (idempotent) → projection (async)
```

The domain core is pure F# with no I/O: an unbalanced transaction is
unrepresentable, enforced by a smart constructor returning `Result`. Persistence
is a hand-rolled event store on PostgreSQL behind a port — optimistic
concurrency, polling subscriptions, and checkpointing are implemented directly
rather than imported, because the mechanics are the point. Idempotency is
enforced by the database via a unique constraint on the idempotency key, so a
retried command cannot double-post.

| Layer | Choice |
|---|---|
| Domain & services | F# |
| Event store | Hand-rolled on Azure Database for PostgreSQL |
| Hosting | Azure Container Apps (API, projection worker, scheduled job) |
| IaC / CI | Terraform, GitHub Actions |
| Frontend | TypeScript + Effect |

## Roadmap

1. **Core billing** — ledger, policy, agreement, rating
2. **Usage & metering** — usage ingestion, metered rating, re-rating replay
3. **Settlement & reconciliation** — simulated provider webhooks (duplicates,
   out-of-order, retries), idempotent processing, drift detection
4. **Dunning & collections** — retry schedules, suspension
5. **Scribe** — an AI layer that drafts pricing policies and explains bills,
   where the deterministic core validates every proposal before anything is
   committed

## Documentation

| Doc | Contents |
|---|---|
| [`docs/decisions.md`](docs/decisions.md) | Dated decision log — what was chosen and why |
| [`docs/ledger.md`](docs/ledger.md) | Double-entry model, revenue recognition, the Transaction type |
| [`docs/architecture.md`](docs/architecture.md) | Event store, consistency boundary, write path, infrastructure |
| [`docs/glossary.md`](docs/glossary.md) | Accounting terms in plain English |
| `docs/policy.md`, `agreement.md`, `rating.md`, `settlement.md`, `scribe.md` | Outlines for the contexts not yet designed in depth |

## License

MIT
