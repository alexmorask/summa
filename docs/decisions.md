# Decision Log

_Append-only. A superseded decision gets a new dated row rather than an edit —
fitting, for a project about immutable records._

## Product & approach

| Date | Decision |
|---|---|
| 2026-07-23 | **Product direction:** combined platform — metering → rating → invoicing → settlement → reconciliation — built in phases, each independently demoable. |
| 2026-07-23 | **Diverge from the standard resource model.** Ledger-first and policy-driven: what a typical billing API stores as mutable resources (Subscription, Invoice, PaymentIntent), Summa computes as projections over an append-only ledger. |
| 2026-07-23 | **AI integration principle:** AI proposes, the deterministic FP core decides. Generated artifacts (pricing policies, explanations, triage notes) must pass pure validators before anything is committed; AI never mutates state directly. |
| 2026-07-24 | **Build the ledger first** as a thin end-to-end vertical slice deployed on Azure, before designing the remaining contexts in depth. Everything depends on the ledger; the ledger depends on nothing. |
| 2026-07-28 | **Project name: Summa.** Sum types (F#), *Summa de Arithmetica* (Pacioli, 1494), and "the total." |
| 2026-07-28 | **Single public monorepo**, docs versioned alongside code, one PR per build-plan stage. |
| 2026-07-28 | **The AI layer is called Scribe.** A scribe drafts and records on behalf of an authority but has no power to decide — the name states the guardrail. (Avoids collision with Microsoft Copilot.) |

## Stack

| Date | Decision |
|---|---|
| 2026-07-23 | **Backend: F#.** Existing fluency means the learning budget goes to event sourcing and infrastructure rather than syntax; first-class Azure SDK support as a .NET language. |
| 2026-07-23 | **Frontend: TypeScript + Effect.** Chosen deliberately as new ground, on a bounded surface (read-only dashboards) where a learning curve can't compromise the core. |
| 2026-07-23 | **Cloud: Azure.** |
| 2026-07-24 | **Event store: hand-rolled on Azure Database for PostgreSQL**, behind an F# persistence port. Optimistic concurrency, polling subscriptions, and checkpointing are implemented directly — building the mechanics is the point. *Rejected:* Marten (C#-idiomatic, hides the mechanics), Equinox (excellent but imports what we want to learn), KurrentDB (no managed Azure offering). *Escape hatch:* Equinox's MessageDb backend is also Postgres, so the port allows a later swap without touching the domain. |
| 2026-07-24 | **Hosting: Azure Container Apps** — API, projection worker, and a scheduled Job for revenue recognition — with Azure Container Registry, Terraform (over Bicep, for transferability), and GitHub Actions. AKS noted as a deliberate later step; ACA already runs on Kubernetes, so containers and code carry over. |
| 2026-07-29 | **SQL migrations: forward-only, plain `.sql` files** in `db/migrations/`, applied manually via psql for now (Stage 5). A correction is a new migration, never a down script — mirrors the ledger's own append-only, reversing-entry rule rather than introducing a separate rollback mechanism. *Rejected:* DbUp/FluentMigrator/Evolve — same reasoning as the event store decision: hand-roll the mechanics rather than import them. |
| 2026-07-29 | **Migration application: hand-rolled `db/migrate.sh`**, refining the row above. A `schema_migrations(version, applied_at)` table tracks which migrations have run; the script applies each pending file inside its own transaction and skips ones already recorded, so re-running it is safe. Still no DbUp/FluentMigrator/Evolve — this only adds the tracking a plain-file approach was missing. |

## Ledger design

| Date | Decision |
|---|---|
| 2026-07-23 | **Double-entry bookkeeping.** Every transaction balances (debits = credits), so the global "all accounts sum to zero" invariant holds inductively — provable by replay, with no global lock. |
| 2026-07-23 | **Append-only.** Corrections are reversing entries, never edits or deletes. Money is stored as integer minor units; every transaction carries an idempotency key. |
| 2026-07-23 | **Revenue recognition is foundational, not a later feature.** Billing, collection, and recognition are three distinct moments that occur at different times and in different orders; a ledger that conflates them cannot be correct. |
| 2026-07-23 | **Recognition is modeled as a Performance Obligation** (ASC 606 term), created at billing. Policy answers *how much*; the Performance Obligation answers *how it becomes earned over time*. |
| 2026-07-23 | **Recognition mechanism: hybrid.** The schedule is fixed when the obligation is created; an idempotent background job posts each recognition entry as its period comes due; recognized-to-date is also computable directly, and both agree by construction. Not-yet-due recognitions are a plan, not a fact, and stay outside the ledger until due. |
| 2026-07-23 | **Tax Payable and Processing Fee Expense** added to the chart of accounts — recorded now, calculation logic deferred. Both are scheduled-settlement obligations: incurred continuously, discharged on recurring remittance dates. |
| 2026-07-23 | **Ledger accounts are internal bookkeeping categories, not connections to financial institutions.** Mapping internal accounts to external ones and proving they agree is reconciliation's job. The chart of accounts is a defined domain model (fixed types, instances parameterized per customer/currency/jurisdiction), not end-user configuration; posting rules are pure functions, not runtime config. |
| 2026-07-24 | **The Transaction is the consistency unit** — self-validating, immutable, appended to one globally-ordered log. Accounts and balances are projections. *Rejected:* one stream per account, which would require cross-stream atomic writes and break the atomic-balancing guarantee. |
| 2026-07-24 | **One Transaction = one business event.** Bill components (service, tax, fee) are entries, not separate transactions, and land in the transaction for the moment they become known. Related transactions are tied by a shared `CorrelationId`, with `CausationId` recording the immediate cause. |
| 2026-07-24 | **The ledger is unconditional** — it records any balanced transaction. Balance-dependent rules (no-negative, no-double-charge) live above it in the consumers that produce transactions. A rule needing a race-proof guarantee promotes that one account to its own stream with a version check; everything else stays a projection. |
| 2026-07-24 | **Write path:** pure `decide` → idempotent append (unique constraint on idempotency key; global identity sequence for ordering) → async projection. No version check needed on this path, since transactions are independent self-balancing facts. |
| 2026-07-28 | **Read surface for the first slice: one projection table** (`account_balances`). Trial balance and revenue-to-date are queries over it, not separate read models. As-of-date historical queries deferred. |

## Open questions

- **Chart of accounts** — credits/promotions, write-offs, multi-currency.
- Remaining recognition timing cases: pay-monthly (arrears), pay-after-delivery.
- Multi-currency — single currency to start, or currency as a dimension?
- Whether Performance Obligation becomes its own bounded context.
- Per-customer account dimensions and as-of-date queries (deferred from the
  first slice).
- Frontend scope per phase.
