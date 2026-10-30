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
- **Currency in the type:** an amount is `Money<Usd>`, so adding amounts in different currencies doesn't compile ([ADR 0004](docs/adr/0004-currency-as-a-type-parameter.md)).
- **The business supplies the ledger's vocabulary:** tag kinds and the chart of accounts come from the business, so the ledger serves any business unchanged ([ADR 0005](docs/adr/0005-the-business-supplies-the-ledgers-vocabulary.md)).
- **Corrections link back:** a posted transaction gets an ID from the ledger, and a reversal records the ID of the transaction it cancels, so nothing is ever edited ([ADR 0006](docs/adr/0006-transaction-identity.md)).
- **Balances as of either time:** balances are computed from entries and can be cut off by when things happened, when the books recorded them, or both, which reproduces any past report ([ADR 0007](docs/adr/0007-as-of-cutoffs.md)).
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

## Backups

The backup is a `pg_dump` snapshot of the `summa` database: consistent as of the moment it ran, but not a replica. Needs the Docker container running (`docker compose up -d`).

```sh
scripts/backup.sh
```

Dumps go to `~/.summa/backups/`, outside the repo, and the newest 7 are kept. After each dump, `scripts/restore-check.sh` restores it into a throwaway database (`summa_restore_check`, dropped afterwards) and checks that it restores without errors, has the same number of transactions as when the dump was taken, and that debits equal credits across all entries. The script prints `restore check passed: N transactions, debits = credits = $X.XX` or the failure, and exits non-zero on failure. Older backups are only deleted after the new one passes; a dump that fails is renamed to `.dump.failed` (with `.count.failed`) and kept for inspection, outside the rotation. Run the check alone on the newest dump with `scripts/restore-check.sh`.

### Nightly job (optional)

The Docker container must be running at 02:00 for the job to work. Run the commands below from the repo root. launchd runs the backup at 02:00, or on wake if the Mac was asleep, and logs to `~/.summa/backups/backup.log`.

```sh
mkdir -p ~/.summa/backups ~/Library/LaunchAgents
sed "s|REPO_PATH|$PWD|g; s|HOME_PATH|$HOME|g" scripts/com.summa.backup.plist \
  > ~/Library/LaunchAgents/com.summa.backup.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.summa.backup.plist
```

Undo it with `launchctl bootout gui/$(id -u)/com.summa.backup`.

### Recovering from a backup

Pick the newest `summa-*.dump` (files ending in `.failed` failed their check and are not listed by this pattern), run `scripts/restore-check.sh "$latest"` on it first, then restore it into a fresh database and point the app at it:

```sh
latest=$(ls ~/.summa/backups/summa-*.dump | tail -1)
scripts/restore-check.sh "$latest"
docker compose exec -T db psql -U summa -d postgres -c 'CREATE DATABASE summa_recovered'
docker compose exec -T db pg_restore --no-owner -U summa -d summa_recovered < "$latest"
export DATABASE_URL=postgres://summa:summa@localhost:5433/summa_recovered
```

### Limits

The dumps sit on the same disk as the database, so losing the machine loses both. Each dump is only as new as the last run, so anything saved since is lost. Recovering to any point in time (continuous archiving of the write-ahead log) is the real-scale design: continuous archiving with point-in-time recovery.
