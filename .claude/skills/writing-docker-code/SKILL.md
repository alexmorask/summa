---
name: writing-docker-code
description: This skill should be used when the user asks to write or draft Dockerfile/container code in the Summa repository — for example "write the API Dockerfile", "containerize the projection worker", "add the .dockerignore", "implement Stage 11", or any request producing new or modified `Dockerfile`(s) or `.dockerignore`. Covers drafting a plan, writing idiomatic multi-stage Docker, and explaining unfamiliar constructs afterward, per this repo's working agreement.
---

## Purpose

Write correct, idiomatic Dockerfiles (and `.dockerignore`) to containerize `Summa.Ledger.Api` and `Summa.Ledger.Projections`, while honoring `.claude/CLAUDE.md`'s working agreement: the owner must understand every line, so this is a plan-then-write, approval-gated process, not a one-shot action.

Scope note: this skill covers container *build* definitions — Dockerfile stages, base images, `.dockerignore` — not the Azure resources that run the resulting images (the registry itself is provisioned in Stage 12; Container Apps and registry-access wiring are Stage 13 — both `writing-terraform-code`) and not the F# inside the image (that's `writing-fsharp-code`, already settled by Stage 9/10's work).

## Process

1. Read `.claude/CLAUDE.md` in full, `docs/build-plan.md`'s Stage 11 entry (Part D), and the "Compute, hosting & delivery" section of `docs/architecture.md` for the task at hand. Do not start a build-plan stage the owner hasn't explicitly approved, and do not reach ahead into Stage 12/13 (Terraform, ACA) work.

2. Draft an implementation plan before writing anything — file by file, what changes and why. Use Plan Mode (`EnterPlanMode` / `ExitPlanMode`) for explicit approval before anything is written, same discipline as `writing-fsharp-code` and `writing-postgresql-code`. Stage 11's scope leaves two design calls genuinely open — resolve both as part of the plan, with a stated reason, rather than defaulting to either shape unexamined:
   - A separate Dockerfile per project (`Summa.Ledger.Api`, `Summa.Ledger.Projections`) vs. one image with an entrypoint switch.
   - Framework-dependent vs. self-contained `dotnet publish` — framework-dependent (the `aspnet`/`runtime` final-stage bases this skill's reference assumes) is the default, but it's not yet a recorded decision; confirm it rather than inheriting it silently.

3. Before finalizing the plan, read `docs/decisions.md`'s 2026-07-30 entry on deferred `BackgroundService`/`IHostedService` adoption for the projection worker. It names Stage 11 (this stage) as one of the two explicit triggers for revisiting that deferral, since containerizing is when the deployment platform starts sending `SIGTERM` before `SIGKILL` on scale-down/redeploy. Raise this with the owner during planning — either it's still out of scope for Stage 11 specifically (worth confirming, not assuming) or it belongs in this stage's plan; don't silently do either.

4. Consult `../references/docker-idioms.md` while planning and writing — multi-stage structure, base image selection, the built-in non-root user, layer-caching order, `.dockerignore` contents, and config-injection.

5. Hold these Summa-specific invariants as non-negotiable, overriding generic Docker advice when they conflict:
   - The runtime (final) stage never contains the SDK — only `dotnet publish` output on an `aspnet`/`runtime` base.
   - No secret, connection string, or environment-specific value is baked into an `ENV`/`ARG` or a copied-in `appsettings.*.json` — configuration reaches the container via environment variables injected at run time (the 12-factor habit `docs/architecture.md` calls out as what keeps the later ACA→AKS jump cheap).
   - The final stage runs as the built-in non-root user (`USER $APP_UID`), not root.
   - Base image tags are pinned to a specific version — never `latest`.
   - Write only what Stage 11 needs: Dockerfile(s), `.dockerignore`, and running both against local Postgres via `docker compose`. No CI workflow, no registry push, no Terraform — those are later stages.

6. After the plan is approved, write the Dockerfile(s) and `.dockerignore`, then verify by running both images against local Postgres via `docker compose up`.

7. After writing, explain every non-obvious Docker construct used, in the chat reply — never in code comments. This is mandatory, not optional polish, same as the F# and Postgres skills' equivalent step.

8. If planning or writing surfaced a genuinely new decision — the entrypoint-switch-vs-two-images call from step 2, the `SIGTERM`-handling call from step 3, a base-image variant choice (chiseled vs. Alpine vs. default), or anything else not already resolved above — invoke `recording-decisions` before considering the task done.

## Additional resources

### Reference files

- **`../references/docker-idioms.md`** — multi-stage structure, base image selection, non-root user, layer caching, `.dockerignore`, config injection, and Docker anti-patterns to flag.

### Related skills

- **`writing-terraform-code`** — sequential, not simultaneous work sessions: Stage 11 finishes and is understood before Stage 12/13 begin. But Stage 13's `container` blocks need to match the already-built Stage 11 images' entrypoint, exposed port, and env-var surface — when that stage is underway, consult the finished Dockerfiles (or re-invoke this skill if the images themselves need to change) rather than guessing at the image contract from the Terraform side.
- **`writing-fsharp-code`** — owns the F# inside the image; this skill assumes that code already exists and is only concerned with how it's packaged and run.
- **`writing-github-actions-code`** — applies from Stage 14b onward, once the CD workflow needs to build and push these images; owns the workflow steps, this skill owns the Dockerfile content they invoke.
- **`recording-decisions`** — invoked from step 8 whenever the plan or the implementation involves a decision not already recorded.
