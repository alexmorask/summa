# Summa — Architecture & Build Decisions

_The ledger vertical slice: infra + build choices. Last updated: 2026-07-28._

## Approach

Build the **Ledger first** as a thin, real, end-to-end vertical slice (a walking
skeleton), deployed on Azure, before deep-diving the other contexts. The ledger
is the foundation; the rest depend on it, not the reverse. Domain design lives in
`ledger.md`; this file holds the infra/build choices.

Thin first slice: domain (decide/evolve) → persistence → balance projection →
deployed on Azure → recognition job. Skip tax/fees/multi-currency for now.

## Decisions

### Event store & database — hand-rolled on PostgreSQL

- Roll our own minimal event store on **Azure Database for PostgreSQL** (managed).
- Rationale: maximizes the event-sourcing learning (we build the mechanics
  rather than importing them), most idiomatic F#, simplest/cheapest infra, and
  full control over the immutable event schema.
- Persistence sits behind a small **F# port (interface)** so the domain never
  depends on the store. Escape hatch: Equinox's MessageDb backend is also
  Postgres, so we can swap to it later without touching the domain.
- We implement the three mechanics ourselves, in their simplest low-load-correct
  form:
  - **Optimistic concurrency** — a version/sequence with a unique constraint;
    reject a stale append, re-read and retry. (Correctness, not throughput.)
  - **Subscriptions** — a background worker that polls the events table for new
    events in order.
  - **Checkpointing** — a checkpoint row recording the last processed position,
    updated in the *same* DB transaction as the projection write so the two
    can't drift.

### Consistency / aggregate boundary — the Transaction is the unit

- The **Transaction is the consistency unit** (the aggregate): created once,
  self-validating (entries must sum to zero), appended, never changed.
- The log is a **single, globally-ordered append-only sequence** of transactions
  (one events table, global sequence number). Total ordering → easy
  subscriptions + checkpointing; the "all accounts sum to zero" invariant holds
  inductively.
- **Accounts / balances are projections** folded from the log, not streams you
  write to. (One stream *per account* was rejected: a transaction spans multiple
  accounts, so per-account streams would require cross-stream atomic writes and
  break the atomic-balancing guarantee.)
- The **ledger is unconditional** — it records any balanced transaction.
  Balance-dependent business rules (no-negative, no-double-charge) live *above*
  the ledger in the consumers that produce transactions (Rating, Agreement),
  backed by idempotency keys and, where truly needed, a check against the balance
  projection. Hard race-proof guards can add optimistic concurrency exactly where
  required, rather than taxing every write.

### Guarded accounts — when a rule needs current balance

The general ledger stays unconditional, but a future rule that depends on an
account's current balance (e.g. "never go negative") needs that balance inside
`decide`. Two ways to source it, differing in whether the check is race-proof:

- **From the balance projection** — just read the row. Convenient, but
  projections are eventually consistent (poll → fold → checkpoint lag), so two
  concurrent writes can both read a stale balance and both pass. Fine for a
  *soft* check (warn if likely overdrawn), not for a hard guarantee.
- **From the account's own event stream** — promote that one account to a real
  aggregate: rehydrate its balance by folding its own events, and make the append
  conditional on its version (optimistic concurrency). The version check makes it
  race-proof; the loser retries against fresh state. Use when the rule must truly
  hold.

Rule of thumb: soft nudge → read the projection; hard invariant → give the
account its own stream. Only the guarded account is promoted; every other account
stays a projection and the general ledger is untouched.

### Compute, hosting & delivery — Azure Container Apps

- **Azure Container Apps (ACA)** hosts the workloads: the **API** as a container
  app, the **projection worker** as an always-on container app, and the
  **recognition job** as a scheduled **Container Apps Job** (cron-triggered,
  run-to-completion — the intended fit for batch/reconciliation work).
- **Azure Database for PostgreSQL** (managed) — event store + projections.
- **Azure Container Registry** — images.
- **Terraform** for IaC (over Bicep, for transferability), in modules so a later
  ACA→AKS swap is localized.
- **GitHub Actions** for CI/CD: build/test F# → build image → push to ACR →
  `terraform apply` → deploy.
- **AKS is a deliberate Phase-2 jump, low-lift by design.** ACA already runs on
  Kubernetes (KEDA scaling, Envoy ingress); containers/code/registry/Postgres
  carry over unchanged. The lift is authoring k8s manifests (Deployment/Service/
  Ingress, CronJob), ingress/TLS, cluster ops, and observability — which is
  itself the k8s learning. Keep apps 12-factor to keep it cheap.

### Source control — single public monorepo

Repo: **`summa`**, public from day one.

Rationale: solo developer (no team-ownership case for splitting); changes routinely
span layers (an endpoint touches F#, Terraform, and CI) and should land as one
atomic commit; one CI pipeline; and for a portfolio piece a reviewer follows *one*
link and sees the whole system — architecture, infra, and reasoning together.

```
summa/
├── README.md                    # portfolio front door
├── docs/                        # these design docs, versioned with the code
│   ├── decisions.md   ledger.md      architecture.md
│   ├── glossary.md
├── src/
│   ├── Summa.Ledger.Domain/       # pure F# core
│   ├── Summa.Ledger.Store/        # Postgres adapter
│   ├── Summa.Ledger.Api/          # HTTP
│   └── Summa.Ledger.Projections/  # worker
├── tests/
│   ├── Summa.Ledger.Domain.Tests/
│   └── Summa.Ledger.Store.Tests/
├── db/migrations/               # SQL
├── infra/                       # Terraform modules
├── .github/workflows/           # CI, then CD
└── docker-compose.yml
```

Projects are namespaced by bounded context (`Summa.Ledger.*`) so Policy, Rating,
and Settlement slot in later without restructuring.

**Docs live in the repo.** Versioning design alongside implementation means a
decision and the code implementing it move together — and for a portfolio piece
the visible reasoning (including rejected alternatives and the decision log) is
worth as much as the code.

**One PR per unit of work**, on a short-lived branch (e.g.
`fix-cd-keyvault-read-access`). This enforces the small-batch discipline,
gives a proper diff view for reviewing agent output, and leaves a public record of
incremental, well-scoped work. Given the "understand every line" mission, the PR
*is* the review checkpoint.

## Open (next decisions)

- Public contract — write side defined (Transaction, PostTransaction,
  TransactionPosted; see `ledger.md`); read/query surface still open (balances,
  trial balance, recognized-to-date, API endpoints)
- Projections / read models
