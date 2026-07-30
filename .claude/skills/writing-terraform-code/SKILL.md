---
name: writing-terraform-code
description: This skill should be used when the user asks to write or draft Terraform/infrastructure code in the Summa repository — for example "write the base infrastructure", "add the Postgres flexible server", "set up the remote state backend", "add the Container Apps environment", "wire up the API and worker container apps", or any request producing new or modified `.tf` files under `infra/`. Covers drafting a plan, writing idiomatic Terraform-on-Azure, and explaining unfamiliar constructs afterward, per this repo's working agreement. Applies to Stage 12 (base infrastructure) and Stage 13 (Container Apps) of the build plan.
---

## Purpose

Write correct, idiomatic Terraform for Summa's Azure infrastructure — resource group, Container Registry, PostgreSQL Flexible Server, remote state backend (Stage 12), and the Container Apps environment/apps (Stage 13) — while honoring `.claude/CLAUDE.md`'s working agreement: the owner must understand every line, so this is a plan-then-write, approval-gated process, not a one-shot action.

Scope note: this skill covers infra *resources* — what gets provisioned and how it's wired (networking, identity, secrets, scaling). It does not cover Dockerfile content (`writing-docker-code` owns that) or CI/CD workflow YAML (`writing-github-actions-code` owns that, from Stage 14 onward). Stage 13's Container Apps run the images Stage 11 builds — coordinate with `writing-docker-code` when an app's `image`/port/entrypoint expectations are in question, rather than guessing at them from the Terraform side alone.

## Process

1. Read `.claude/CLAUDE.md` in full, `docs/build-plan.md`'s Stage 12 and Stage 13 entries (Part D) and their acceptance criteria, and the relevant sections of `docs/architecture.md` (Compute, hosting & delivery decision) and `docs/decisions.md` (Stack table) for the task at hand. Do not start a build-plan stage the owner hasn't explicitly approved, and do not start Stage 13 while still finishing Stage 12.

2. Draft an implementation plan before writing anything — resource by resource (or file by file), what gets created and why. Use Plan Mode (`EnterPlanMode` / `ExitPlanMode`) for explicit approval before anything is written, same discipline as `writing-fsharp-code` and `writing-postgresql-code`.

3. Before finalizing the plan, read `../references/terraform-idioms.md` in full — in particular its secret-handling section if the task touches the Postgres admin credential, registry auth, or Stage 13's connection-string wiring. Getting these right at plan time avoids writing a working-but-insecure `apply` and then reworking it.

4. Hold these Summa-specific invariants as non-negotiable, overriding generic Terraform-on-Azure advice when they conflict:
   - **Use `infra/modules/` from Stage 12 onward, not flat `.tf` files** — `docs/architecture.md`'s "in modules so a later ACA→AKS swap is localized" line is a recorded decision (confirmed `docs/decisions.md`, 2026-07-30), not speculative scaffolding to second-guess. Decide module boundaries as part of the stage plan (e.g. one module per resource concern), with a root `infra/*.tf` composing them.
   - Remote state from Stage 12 onward — the one-time bootstrap apply that creates the state storage account is the only step allowed to run with local state.
   - No secret (password, connection string, access key) ever appears as a literal in a committed `.tf` file — see the reference file's secret-handling section for the concrete per-resource pattern (managed identity + role assignment for registry access, Key Vault references or `TF_VAR_*` for the Postgres admin password).
   - Smallest viable tier/SKU per the build plan's explicit instruction ("smallest tier" for Stage 12's Postgres server) — don't upsize preemptively.
   - Provider version pinned (`~> 4.2` minimum) in a `required_providers` block — never an unbounded version constraint.
   - The API container app gets `ingress`; the worker does not, matching `docs/build-plan.md`'s Stage 13 "no ingress, always-on" description verbatim.
   - **A budget alert is part of Stage 12's acceptance criteria** (`docs/build-plan.md`) — don't consider Stage 12 done without one.

5. After the plan is approved, write the Terraform (and, where applicable, coordinate with `writing-docker-code` for image/port expectations Stage 13's `container` blocks depend on).

6. Run `terraform validate` after writing, before proposing `terraform plan`/`apply` — this catches syntax and internal-consistency errors cheaply, before anything touches a real subscription. Never run `terraform apply` without the owner's explicit go-ahead on that specific apply; a plan approval is not an apply approval, since `apply` costs real money and can't be casually reverted like a code diff.

7. After writing, explain every non-obvious Terraform/HCL construct used, in the chat reply — never in code comments. This is mandatory, not optional polish, same as the F# and Postgres skills' equivalent step. Cover things like: what state actually stores vs. what's read live from Azure at plan time, why the resource dependency graph implies the apply order it does, and any managed-identity/role-assignment plumbing that isn't self-explanatory from the resource names alone.

8. If planning or writing surfaced a genuinely new decision not already covered by step 4's resolved invariants — a naming scheme, a SKU choice, whether `public_network_access_enabled` stays at its default, a secrets-storage approach (Key Vault vs. `TF_VAR_*`) — invoke `recording-decisions` before considering the task done.

## Additional resources

### Reference files

- **`../references/terraform-idioms.md`** — provider version pinning, remote state backend mechanics, resource naming/tagging, secret handling (registry, Postgres admin password, Container App secrets), module-vs-flat-file structure, Container Apps revision/ingress/scaling basics, and Terraform-on-Azure anti-patterns.

### Related skills

- **`writing-docker-code`** — sequential, not simultaneous work sessions: Stage 11 finishes first. Stage 13's `container` blocks then need to match the already-built images' entrypoint, exposed port, and env-var surface — consult the finished Dockerfiles for that contract rather than guessing at it from the Terraform side.
- **`writing-github-actions-code`** — applies from Stage 14b onward, once the CD workflow needs to invoke `terraform plan`/`apply`; owns the workflow steps that call into this skill's HCL, not the HCL itself.
- **`recording-decisions`** — invoked from step 8 whenever a new infra decision surfaces (naming scheme, SKU choice, secrets-storage approach).
