# Summa

Summa is a billing and payments platform built on a double-entry ledger. Every financial event is a balanced transaction in an append-only log, and everything else (balances, invoices, earned revenue, tax owed) is computed from that log.

> A complete billing and payments platform can be built on a double-entry ledger as its single foundation, by composing small, pure building blocks. Every business concept the platform offers is either recorded as balanced transactions or computed from them.

## The idea in one transaction

A customer pays \$21.60 for a \$20.00 monthly plan plus 8% tax. Summa records it as one transaction with three entries:

| Account                         | Debit  | Credit |
| ------------------------------- | -----: | -----: |
| Payment Provider Clearing       | $21.60 |        |
| Deferred Revenue: Subscriptions |        | $20.00 |
| Sales Tax Payable               |        |  $1.60 |

Debits equal credits, or the transaction is rejected. None of the \$21.60 is revenue yet: \$20.00 is service still owed to the customer, earned day by day over the month, and \$1.60 is owed to the government and never becomes revenue. Billing, collection and revenue recognition stay separate, and every number Finance sees can be traced back to entries like these.

## The reference customer

Summa serves one concrete business per version, while the ledger core stays general: it knows nothing about which business it serves.

**v1 bills for an AI company with two products,** sharing one ledger, one customer and one payment method:

- **Subscriptions:** individual and team plans, seats, upgrades and downgrades with proration, renewals, failed-payment retries, invoices and tax.
- **API usage:** prepaid credits, metered usage, auto-recharge and spend limits.

**v2 adds a marketplace** on the same core: collecting from buyers, taking a fee and paying out sellers.

## Status

Early. The ledger core (M1, ledger rules) is in progress. Nothing is deployed; Summa runs locally until the subscription product is complete.

## Roadmap

| Phase                    | Milestones |
| ------------------------ | ---------- |
| **Ledger core**          | M1 Ledger rules · M2 Ledger storage · M3 Worker and playground |
| **Subscription product** | M4 Buy a plan · M5 Time and earned revenue · M6 Invoices and tax · M7 Plan and seat changes · M8 Failed payments, cancellation, refunds · M9 Finance tools |
| **Go live**              | First public demo |
| **API product**          | M10 Prepaid credits · M11 Usage and pricing · M12 Spend limits and auto-recharge · M13 Credit expiry and tax on credits |
| **Finishing v1**         | M14 Reconciliation · M15 Re-price and replay · M16 Enterprise commitments (stretch) |

Each milestone adds one idea, ends with something visibly working, and starts with a design doc covering scale, trade-offs and failure modes.

## Tech

- **Rust** backend ([ADR 0001](docs/adr/0001-rust-with-go-fallback.md)), with a **TypeScript** front end to come.
- **A modular monolith:** one app, with each module in its own crate and boundaries checked by the compiler ([ADR 0002](docs/adr/0002-modular-monolith.md)).
- **Time is an input:** code never reads the system clock, so a simulated clock can fast-forward renewals and revenue recognition ([ADR 0003](docs/adr/0003-time-as-an-input.md)).
- **PostgreSQL** for append-only storage, from M2.
- **OpenTelemetry** for metrics, traces and logs, including ledger-specific signals like whether the trial balance still balances.
- **Simulated** payment provider, tax provider and clock, so the demo can decline cards, change tax locations and fast-forward time.

## How it's built

Summa is built with AI agents (Claude Code) under my direction. I own the product, the domain model and the design decisions; agents write the code from plans I approve, and I review every change until I can explain it. Decisions are recorded in [`docs/adr/`](docs/adr/).

## Running it

Requires Rust 1.94 or later.

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```
