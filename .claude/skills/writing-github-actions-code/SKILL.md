---
name: writing-github-actions-code
description: This skill should be used when the user asks to write or draft GitHub Actions / CI-CD workflow code in the Summa repository — for example "write the CI workflow", "add the build-and-test workflow", "write the CD workflow", "wire up the terraform apply step", "add OIDC login to the workflow", or any request producing new or modified files under `.github/workflows/`. Covers drafting a plan, writing idiomatic Actions YAML, and explaining unfamiliar constructs afterward, per this repo's working agreement.
---

## Purpose

Write correct, idiomatic GitHub Actions workflows for the Summa CI/CD pipeline — first the CI workflow (restore/build/test on PR), then, once that's green, the CD workflow (build images, push to ACR, `terraform apply`, deploy) — while honoring `.claude/CLAUDE.md`'s working agreement: the owner must understand every line, so this is a plan-then-write, approval-gated process, not a one-shot action.

Scope note: this skill covers `.github/workflows/*.yml` content — triggers, jobs, steps, auth, caching, and how a workflow *calls* `terraform plan`/`apply` or `docker build`. It does not own the Terraform HCL itself (`writing-terraform-code`) or the Dockerfile content (`writing-docker-code`) — draft those together when the task spans both, rather than treating them as sequential, separate tasks.

## Process

1. Read `.claude/CLAUDE.md` in full, and the relevant sections of `docs/build-plan.md` (Stage 14a/14b and their acceptance criteria), `docs/architecture.md` (Compute, hosting & delivery — the ACA/ACR/Terraform/GitHub Actions decision), and `docs/decisions.md` (Stack table) for the task at hand. Do not start a build-plan stage the owner hasn't explicitly approved, and do not start the *next* stage while finishing the current one.

2. Confirm which of Stage 14a (CI, `ci.yml`) or 14b (CD, `cd.yml`) is in scope before planning anything — this split is already decided (`docs/decisions.md`, 2026-07-30), not a call to make fresh. 14b must not be started until 14a is merged and green, even if the owner asks for "the GitHub Actions stage" as one request.

3. Draft an implementation plan before writing anything — workflow file by workflow file, job by job, what changes and why. Use Plan Mode (`EnterPlanMode` / `ExitPlanMode`) for explicit approval before anything is written, same discipline as `writing-fsharp-code` and `writing-postgresql-code`.

4. Before finalizing the plan, read `../references/github-actions-idioms.md` in full — in particular its callout on OIDC authentication if the task touches Azure login at all. That callout describes the two easiest things to get wrong on the first pass (`permissions: id-token: write`, and no `client-secret` input existing in the OIDC path) — resolve them during planning, not after writing a workflow that silently drifts back to a long-lived credential.

5. Hold these Summa-specific invariants as non-negotiable, overriding generic Actions advice when they conflict:
   - **Azure auth is OIDC federated credentials via `azure/login`, never `AZURE_CREDENTIALS` or any client-secret blob** — this is a recorded decision (`docs/build-plan.md` Stage 14b, confirmed `docs/decisions.md` 2026-07-30), not a style preference.
   - **CI (14a) triggers on `pull_request` and on `push` to `main`; CD (14b) triggers on `push` to `main` only** — matching `docs/build-plan.md`'s Stage 14a scope exactly (CI also runs post-merge, so `main` itself is proven green after every merge, not just each PR in isolation). CD must never be reachable from a forked/untrusted PR, since it holds deploy credentials.
   - **`terraform apply` never runs against local/default state in CI** — Stage 12's remote state backend is a hard prerequisite; if it isn't provisioned yet, that's a blocker to raise, not to work around.
   - **Plan before apply, always** — a visible `terraform plan` output precedes any `apply`, even on the unattended CD path; no bare `-auto-approve` with nothing preceding it.
   - **CI must be green before CD is attempted** — per `docs/build-plan.md`, don't build the CD workflow in the same session as an unproven CI workflow.

6. After the plan is approved, write the workflow YAML (and, where applicable, coordinate with `writing-terraform-code` for the Terraform steps' surrounding HCL, or `writing-docker-code` for the image-build steps' Dockerfiles).

7. After writing, explain every non-obvious Actions/YAML construct used, in the chat reply — never in code comments. This is mandatory, not optional polish, same as the other writing-*-code skills' equivalent step.

8. If planning or writing surfaced a genuinely new decision not already covered by step 5's resolved invariants or step 2's already-decided workflow split — environment/branch-protection scoping, a concurrency-group strategy, anything else not already resolved — invoke `recording-decisions` before considering the task done.

## Additional resources

### Reference files

- **`../references/github-actions-idioms.md`** — OIDC federated credential setup, trigger scoping (PR vs. push to main), job/step structure for the .NET/F# solution, NuGet caching, safe Terraform-in-CI patterns (remote state, plan-then-apply, concurrency guards), environment/secrets wiring, and CI/CD anti-patterns to flag. Not shared with a reviewing-side skill (none exists yet for this technology) — owned by this skill alone, unlike the F#/Postgres idiom files.

### Related skills

- **`writing-terraform-code`** — applies simultaneously whenever the CD workflow's `terraform apply` step is being wired up; owns the HCL/module content this skill doesn't duplicate, this skill owns the workflow steps that invoke it (init, plan, apply, state backend config as passed to the workflow).
- **`writing-docker-code`** — applies simultaneously whenever the CD workflow builds/pushes images; owns the Dockerfile content, this skill owns the `docker build`/`push`-to-ACR steps and their auth.
- **`recording-decisions`** — invoked from step 8 whenever a new decision surfaces.
