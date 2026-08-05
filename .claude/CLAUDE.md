# Summa — working notes for Claude Code

An event-sourced, double-entry billing ledger in F#, on Azure. This file is the
standing brief for every session in this repo.

## Prime directive

**The owner must understand every line.** This project exists for comprehension,
not velocity. A stage that produces working code the owner cannot explain has
failed. Prefer the smallest change that teaches the concept.

## Working agreement

1. **One build-plan stage per session.** See `docs/build-plan.md`. Do not start
   the next stage; finish, explain, stop.
2. **Explain before writing.** Describe what you intend to add, file by file, and
   why. Wait for approval before writing code.
3. **No speculative abstraction.** No interfaces, layers, helpers, config, or
   generalization "for later." Only what the current stage requires. (The event
   store port is the one deliberate exception — it is a design decision.)
4. **No unrequested files.** No READMEs, sample data, editor config, or tooling
   unless the stage asks for it.
5. **Small diffs.** If a change will exceed ~5 files or a few hundred lines,
   stop and propose a split.
6. **Explain the unfamiliar.** Any F#, SQL, Terraform, Docker, or GitHub Actions
   construct that isn't obvious — explain it in chat, not in code comments.
7. **Update the docs in the same change.** A decision made while coding gets a
   row in `docs/decisions.md`; a design change updates the relevant doc.

## Domain invariants — never violate these

These are correctness properties, not preferences. If a change appears to
require breaking one, stop and raise it.

- **Every transaction balances.** Total debits = total credits, always. An
  unbalanced `Transaction` must be unrepresentable — enforced by a smart
  constructor returning `Result`, not by a caller remembering to check.
- **The ledger is append-only.** Never edit or delete an entry. Corrections are
  *reversing entries* (a new transaction with inverse entries, linked via
  `CausationId`).
- **Money is integer minor units** (`int64` cents). Never float, never decimal.
  Any division must have an explicit, total-preserving rounding rule — the
  remainder has to land somewhere and the parts must sum exactly to the whole.
- **Idempotency is enforced by the database**, via a unique constraint on
  `idempotency_key`. Retrying a command must never double-post.
- **The domain core is pure.** No I/O in `Summa.Ledger.Domain` — no database, no
  HTTP, no clock, no `Guid.NewGuid()` buried in a decision function. Effects live
  at the edges.
- **Accounts are projections, not state to mutate.** A balance is a fold over the
  transaction log. The log is the only source of truth; every read model must be
  destroyable and rebuildable from it.
- **Business time ≠ system time.** `OccurredAt` (when it economically happened)
  is distinct from the log's record order and `recorded_at`.

## Orientation

| Doc | Read it when |
|---|---|
| `docs/build-plan.md` | Starting any stage — it defines scope and acceptance |
| `docs/ledger.md` | Touching the domain model, transactions, or recognition |
| `docs/architecture.md` | Touching persistence, the write path, or infrastructure |
| `docs/decisions.md` | Wondering why something is the way it is |
| `docs/glossary.md` | Encountering an unfamiliar accounting term |

Design decisions are already made and recorded. Do not relitigate them mid-stage;
if one seems wrong, say so and let the owner decide.

## Conventions

- **Projects are namespaced by bounded context:** `Summa.Ledger.Domain`,
  `Summa.Ledger.Store`, `Summa.Ledger.Api`, `Summa.Ledger.Projections`,
  `Summa.Recognition.Domain`, `Summa.Recognition.Store`,
  `Summa.Recognition.Job`. Policy, Rating, and Settlement will follow the
  same pattern.
- **F# file order matters** — declaration order within a project is significant;
  keep it deliberate.
- **Model with types.** Prefer discriminated unions and records that make illegal
  states unrepresentable over runtime validation.
- **`Result` over exceptions** for domain errors. Exceptions are for genuinely
  exceptional infrastructure failures.
- **Use the domain's vocabulary.** Standard accounting terms (Performance
  Obligation, Deferred Revenue, Accounts Receivable) — not invented synonyms.

## Status

Through Stage 15 of `docs/build-plan.md`, plus the Stage 8 integration-test
addendum (`Summa.Ledger.Api.Tests.Integration`) and the BackgroundService/
IHostedService addendum for the projection worker (`docs/decisions.md`,
2026-07-31). Stages 12–14b are applied and live on Azure: a merge to `main`
builds and pushes images, runs database migrations, and deploys via
`terraform apply`, ending with a smoke test against the real deployed API.
`Summa.Ledger.Api` now requires Entra ID authentication (JWT Bearer, App
Roles) on every route except `/health` — see `docs/decisions.md` for the
identity model. Stage 16 (the recognition job, `Summa.Recognition.Job`) is
written and tested locally against real Postgres, but not yet deployed — its
Terraform/CD wiring is a separate, still-pending change.
Implementation is underway; update this section as the slice progresses.

- Build: `dotnet build`
- Test: `TEST_CLIENT_ID=... TEST_CLIENT_SECRET=... dotnet test` (values from
  `infra`'s `api_auth_test_client_id`/`api_auth_test_client_secret` outputs)
- Local Postgres: `docker compose up -d`
- Apply migrations: `./db/migrate.sh`
- Run the API: `dotnet run --project src/Summa.Ledger.Api`
- Run the projection worker: `dotnet run --project src/Summa.Ledger.Projections`
- Run the recognition job: `dotnet run --project src/Summa.Recognition.Job`
- Rebuild the projection: `./db/rebuild.sh` (then restart the worker)
- Run everything containerized: `docker compose up --build`
