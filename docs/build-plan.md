# Summa — Build Plan (Ledger Vertical Slice)

_Sequenced, small-batch tasks for building the ledger slice with Claude Code._
_Design decisions live in `ledger.md` and `architecture.md`. Last updated: 2026-07-28._

## Prime directive

**Understand every line.** The goal of this project is comprehension, not
velocity. Each stage below is deliberately scoped to a handful of files so the
diff is reviewable in one sitting. Never let an agent run ahead of your
understanding — a stage that produced code you can't explain is not complete,
regardless of whether it works.

## Working agreement with Claude Code

Rules to hold the agent to on every stage:

1. **One stage at a time.** Do not start the next stage in the same session.
   Finish, review, understand, commit — then move on.
2. **Explain before writing.** Ask the agent to describe what it intends to
   write and why, and wait for your go-ahead before it writes.
3. **No speculative abstraction.** No interfaces, layers, helpers, or config
   "for later." Only what this stage needs. (Exception: the event store port,
   which is an explicit design decision.)
4. **No unrequested files.** No READMEs, no extra tooling, no editor config, no
   sample data unless the stage asks for it.
5. **Small diffs.** If a stage looks like it will exceed ~5 files or a few
   hundred lines, stop and split it.
6. **Explain the unfamiliar.** Any F#, SQL, or Terraform construct you don't
   recognize — make the agent explain it in the chat, not in code comments.
7. **You write the commit message.** If you can't summarize the change
   accurately, you don't understand it yet.

A useful prompt preamble:

> Before writing any code, explain what you plan to add, file by file, and why.
> Keep it minimal — only what this stage needs, no speculative abstractions or
> extra files. After I approve, write it, then walk me through each file and
> explain anything non-obvious.

## Review checkpoint (run after every stage)

- [ ] I can explain what each new file does and why it exists.
- [ ] I can explain every non-obvious line (especially F# operators, SQL
      semantics, Terraform resource arguments).
- [ ] I could delete this code and rewrite it myself from the design docs.
- [ ] Nothing was added that this stage didn't require.
- [ ] Tests (where applicable) pass, and I understand what they assert.
- [ ] Committed with a message I wrote.

---

## Part A — Domain (pure F#, no infrastructure)

Build the functional core first. No database, no HTTP, no cloud. Everything here
is pure and unit-testable, and it's the part where the FP learning lives.

### Stage 0 — Repo setup

**Goal:** the `summa` repo exists, public, with the docs in it.

**Scope:** create the public GitHub repo `summa`; commit `docs/` (these design
files) and a short README stating what Summa is and the name's origin. No code.

**Acceptance:** repo is public; docs render on GitHub; `main` is protected (or at
least you've decided your PR convention — one PR per stage from here on).

**Understand:** nothing technical — this stage exists so the design is committed
*before* the code, and so every subsequent stage is a reviewable PR.

### Stage 1 — Solution skeleton

**Goal:** an empty but correctly structured .NET solution that builds.

**Scope:** `.gitignore`, solution file, one library project
(`Summa.Ledger.Domain`), one test project (`Summa.Ledger.Domain.Tests`). Nothing
else.

**Acceptance:** `dotnet build` and `dotnet test` succeed with zero real tests.

**Understand:** what each project file declares, why F# compilation order is
explicit (file order in `.fsproj` matters — this is a real F# concept worth
knowing early), and why projects are namespaced by bounded context
(`Summa.Ledger.*`) so Policy/Rating/Settlement slot in later.

### Stage 2 — Core domain types

**Goal:** the vocabulary of the ledger as types.

**Scope:** one file — `AccountId`, `Money`, `Direction`, `Entry`, `Transaction`,
`LedgerError`. Types only, no behavior yet. See `ledger.md` § Transaction.

**Acceptance:** it compiles; nothing else.

**Understand:** discriminated unions vs records; why `Money` is `int64` minor
units and never a float or decimal; why an invalid state should be
unrepresentable.

### Stage 3 — Transaction smart constructor + tests

**Goal:** make an unbalanced transaction impossible to construct.

**Scope:** `Transaction.create` returning `Result<Transaction, LedgerError>`,
plus unit tests covering: balanced 2-entry, balanced 3-entry (the
service/tax/fee shape), unbalanced, fewer than 2 entries, non-positive amount.

**Acceptance:** tests pass; every `LedgerError` case has a test.

**Understand:** `Result` as an alternative to exceptions; why validation lives in
a constructor rather than a separate validator; why this makes `decide` trivial.

### Stage 4 — Commands, events, and `decide`

**Goal:** the pure write-path decision function.

**Scope:** `PostTransaction` command, `TransactionPosted` event, and
`decide : State -> Command -> Result<Event list, LedgerError>`. See
`architecture.md` § write path. State is `unit` for now — that's deliberate.

**Acceptance:** tests prove a valid command yields exactly one event and an
invalid one yields the right error.

**Understand:** why `decide` is pure and I/O-free; why State is trivial today
(the unconditional-ledger decision) and where a balance-dependent guard would
plug in later.

---

## Part B — Persistence (the hand-rolled event store)

### Stage 5 — Local Postgres + event table schema

**Goal:** somewhere to append to, running locally.

**Scope:** `docker-compose.yml` with Postgres, and one SQL migration creating
the events table: global sequence (identity), transaction id, occurred_at,
correlation_id, causation_id, idempotency_key (**unique**), payload (jsonb),
recorded_at.

**Acceptance:** container runs; table exists; you can insert manually via psql
and see the unique constraint reject a duplicate idempotency key.

**Understand:** why `BIGINT GENERATED ALWAYS AS IDENTITY` gives us global
ordering; why the unique constraint on `idempotency_key` *is* our idempotency
mechanism; why `occurred_at` (business time) is separate from `recorded_at`
(system time); jsonb vs json.

### Stage 6 — Event store port + Postgres adapter

**Goal:** append and read events, with the domain unaware of Postgres.

**Scope:** the `IEventStore` port (`Append`, `ReadFrom`), a Postgres
implementation using Npgsql, and serialization of `Transaction` to/from jsonb.
Integration tests against the local container.

**Acceptance:** appending returns `Appended seq`; appending the same
idempotency key twice returns `Duplicate existingId` and leaves exactly one row.

**Understand:** `ON CONFLICT DO NOTHING ... RETURNING` semantics; why
idempotency is enforced by the database rather than application code; why the
port exists (swappability, and testing without a database).

### Stage 7 — Command handler (wiring core to edge)

**Goal:** the imperative shell around the pure core.

**Scope:** `handle : IEventStore -> Command -> Async<Result<Guid, LedgerError>>`
per `architecture.md`, plus tests using an in-memory `IEventStore`.

**Acceptance:** valid command appends and returns the id; duplicate returns the
*existing* id as a success, not an error.

**Understand:** functional core / imperative shell; why a duplicate is a success
(idempotent semantics) rather than a 409.

---

## Part C — API and projections

### Stage 8 — HTTP API: write endpoint

**Goal:** post a transaction over HTTP.

**Scope:** a minimal F# web app (Falco or Giraffe — pick one and stick with it),
`POST /transactions` and `GET /health`. JSON request → command → handler →
response. Error mapping: `LedgerError` → 400 with a useful body.

**Acceptance:** curl the invoice transaction from `ledger.md` and get a
transaction id; repeat with the same idempotency key and get the same id back.

**Understand:** the chosen framework's routing/handler model; where JSON
deserialization can fail and how that differs from domain validation failure.

### Stage 9 — Projection: `account_balances` + checkpoint

**Goal:** the read model, and the subscription/checkpoint machinery.

**Scope:** migration for `account_balances` and `projection_checkpoint`; a
worker that polls `ReadFrom(lastSeq)`, folds entries into balances, and advances
the checkpoint **in the same database transaction**.

**Acceptance:** post the three transactions from the worked example (invoice,
payment, recognition); balances land at Cash 120, AR 0, Deferred Revenue 110,
Revenue 10. Kill the worker mid-run and restart — no double-counting, no gaps.

**Understand:** why the projection write and checkpoint advance must be atomic;
what "eventual consistency" means for a read right after a write; how the fold
applies debits and credits (and the sign convention you chose).

### Stage 10 — Read endpoints + rebuild

**Goal:** query the read model, and prove it's disposable.

**Scope:** `GET /accounts/{id}/balance`, `GET /trial-balance` (sum of all
balances + a `balanced: true/false` assertion), and a rebuild path that truncates
the projection, resets the checkpoint, and replays from sequence 0.

**Acceptance:** trial balance returns zero-sum after the worked example; after a
rebuild, balances are byte-identical.

**Understand:** why the projection can be destroyed and rebuilt but the log
cannot; why the trial balance is the headline correctness demonstration.

---

## Part D — Cloud (Azure, Terraform, CI/CD)

Deliberately after a working local slice: deploying something that already works
isolates infrastructure problems from application problems.

### Stage 11 — Containerize

**Scope:** Dockerfile for the API, Dockerfile for the worker (or one image with
an entrypoint switch — decide and note why), `.dockerignore`. Run both against
local Postgres via compose.

**Understand:** multi-stage builds; why the runtime image shouldn't contain the
SDK; how configuration reaches the container (env vars — the 12-factor habit
that keeps the later AKS jump cheap).

### Stage 12 — Terraform: base infrastructure

**Scope:** resource group, Azure Container Registry, Azure Database for
PostgreSQL (smallest tier), and remote state backend. No app resources yet.

**Acceptance:** `terraform apply` creates it; you can connect to the cloud
Postgres and run migrations.

**Understand:** providers, state (and why remote state matters), resource
dependencies, what's stored in state vs. what's read live. Set a budget alert.

### Stage 13 — Terraform: Container Apps

**Scope:** Container Apps environment, the API app (with ingress), the worker
app (no ingress), secrets/connection string wiring, and registry access.

**Acceptance:** the deployed API answers `/health` on a public URL; posting a
transaction updates balances read back through the deployed API.

**Understand:** ACA revisions and scaling; how secrets are injected; how the
worker differs from the API (no ingress, always-on).

### Stage 14 — GitHub Actions: CI, then CD

**Scope:** first a CI workflow (restore, build, test on PR). Only once that is
green, a CD workflow (build images, push to ACR, apply Terraform, deploy).
Two separate stages if the diff gets large.

**Understand:** why CI comes first; how the workflow authenticates to Azure (use
OIDC federated credentials, not a long-lived secret); why `terraform apply` in CI
needs remote state.

---

## Part E — Revenue recognition (the first real consumer)

This is where the ledger proves it's a foundation: a separate component produces
transactions and the ledger just records them.

### Stage 15 — Performance Obligation + schedule generation

**Scope:** pure domain — a Performance Obligation type and a function generating
its recognition schedule (straight-line, N periods), with deterministic entry ids
(`obligation id + period index`). Unit tests including uneven division (e.g.
$100 over 3 months — decide and test the rounding rule; the remainder must land
somewhere and the total must be exact).

**Understand:** why the schedule is deterministic; why rounding must be explicit
and total-preserving in money math.

### Stage 16 — Recognition job (idempotent catch-up)

**Scope:** a job that computes which recognition entries should exist by now and
posts any that are missing, using the deterministic ids as idempotency keys.
Runs as an ACA scheduled Job (Terraform) after it works locally.

**Acceptance:** run it twice in a row — the second run posts nothing. Run it
after skipping a period — it catches up correctly. Balances match the worked
example after 1, 2, and 12 periods.

**Understand:** why "compute what should exist, post what's missing" is more
robust than "post this period's entry"; how this reuses the ledger's idempotency
rather than inventing its own.

---

## After the slice

Once Stage 16 is green and deployed, the ledger is a real foundation and the next
context (Policy, then Agreement, then Rating) can be designed and built on top.
Revisit the deferred items in `decisions.md` § Open Questions — multi-currency,
per-customer account dimensions, as-of-date queries — as they become necessary,
not before.
