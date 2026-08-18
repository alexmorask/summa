---
name: writing-dockerfiles
description: This skill should be used when the user asks to write or draft Dockerfile/container code in the Summa repository — for example "write the API Dockerfile", "containerize the projection worker", "add the .dockerignore", "containerize the recognition job", or any request producing new or modified `Dockerfile`(s) or `.dockerignore`. Covers drafting a plan, writing idiomatic multi-stage Docker, and explaining unfamiliar constructs afterward, per this repo's working agreement.
---

## Purpose

Write correct, idiomatic Dockerfiles (and `.dockerignore`) to containerize Summa's F# services (currently `Summa.Ledger.Api`, `Summa.Ledger.Projections`, `Summa.Recognition.Job`), while honoring `.claude/CLAUDE.md`'s working agreement: the owner must understand every line, so this is a plan-then-write, approval-gated process, not a one-shot action.

Scope note: this skill covers container *build* definitions — Dockerfile stages, base images, `.dockerignore` — not the Azure resources that run the resulting images (provisioning the registry and Container Apps, and registry-access wiring, is `writing-terraform`'s job) and not the F# inside the image (`writing-fsharp`'s job — this skill assumes that code already exists).

## Process

1. Read `.claude/CLAUDE.md` in full, and the "Compute, hosting & delivery" section of `docs/architecture.md` for the task at hand. Scope is whatever the owner asked for directly, or whatever a Linear Issue handed off via `implementing-linear-issues` specifies — don't reach ahead into Terraform/Container Apps work, which is `writing-terraform`'s job.

2. Draft an implementation plan before writing anything — file by file, what changes and why. Use Plan Mode (`EnterPlanMode` / `ExitPlanMode`) for explicit approval before anything is written, same discipline as `writing-fsharp` and `writing-sql`. Two design calls were already settled by the existing Dockerfiles (`src/Summa.Ledger.Api/Dockerfile`, `src/Summa.Ledger.Projections/Dockerfile`, `src/Summa.Recognition.Job/Dockerfile`) — follow them as precedent rather than re-litigating, unless the task has a genuine reason to deviate:
   - A separate Dockerfile per project, not one image with an entrypoint switch.
   - Framework-dependent `dotnet publish` (an `aspnet`/`runtime` final-stage base) — not self-contained.

3. Consult `../references/docker-idioms.md` while planning and writing — multi-stage structure, base image selection, the built-in non-root user, layer-caching order, `.dockerignore` contents, and config-injection.

4. If the plan needs to verify a base-image tag/digest is current, or registry-auth behavior that `../references/docker-idioms.md` doesn't settle, invoke `verifying-current-documentation` before finalizing the plan — the authoritative source for anything Microsoft-specific (e.g. the `mcr.microsoft.com` base images this repo uses).

5. Hold these Summa-specific invariants as non-negotiable, overriding generic Docker advice when they conflict:
   - The runtime (final) stage never contains the SDK — only `dotnet publish` output on an `aspnet`/`runtime` base.
   - No secret, connection string, or environment-specific value is baked into an `ENV`/`ARG` or a copied-in `appsettings.*.json` — configuration reaches the container via environment variables injected at run time (the 12-factor habit `docs/architecture.md` calls out as what keeps the later ACA→AKS jump cheap).
   - The final stage runs as the built-in non-root user (`USER $APP_UID`), not root.
   - Base image tags are pinned to a specific version — never `latest`.
   - Write only what the task needs: Dockerfile(s), `.dockerignore`, and verifying the image(s) against local Postgres via `docker compose`. No CI workflow, no registry push, no Terraform — those are `writing-github-actions-workflows`'s and `writing-terraform`'s jobs.

6. After the plan is approved, write the Dockerfile(s) and `.dockerignore`, then verify by running the image(s) against local Postgres via `docker compose up`.

7. After writing, explain every non-obvious Docker construct used, in the chat reply — never in code comments. This is mandatory, not optional polish, same as the F# and Postgres skills' equivalent step.

8. If planning or writing surfaced a genuinely new decision — a deviation from the existing per-project-Dockerfile/framework-dependent precedent, a base-image variant choice (chiseled vs. Alpine vs. default), or anything else not already resolved above — invoke `recording-decisions` before considering the task done.

## Additional resources

### Reference files

- **`../references/docker-idioms.md`** — multi-stage structure, base image selection, non-root user, layer caching, `.dockerignore`, config injection, and Docker anti-patterns to flag.

### Related skills

- **`writing-terraform`** — a Container App's `container` blocks need to match this skill's images' entrypoint, exposed port, and env-var surface — consult the finished Dockerfiles for that contract (or re-invoke this skill if the images themselves need to change) rather than guessing at it from the Terraform side.
- **`writing-fsharp`** — owns the F# inside the image; this skill assumes that code already exists and is only concerned with how it's packaged and run.
- **`writing-github-actions-workflows`** — owns the workflow steps that build and push these images to ACR; this skill owns the Dockerfile content they invoke.
- **`verifying-current-documentation`** — invoked from step 4 when the idioms reference doesn't settle current base-image or registry-auth behavior.
- **`recording-decisions`** — invoked from step 8 whenever the plan or the implementation involves a decision not already recorded.
