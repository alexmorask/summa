# GitHub Actions idiom reference

Condensed, cited reference for writing GitHub Actions / CI-CD workflow YAML in this repository. Prefer a concrete rule over vague advice. This file lives at the `skills/` level (`../references/github-actions-idioms.md` from `writing-github-actions-code`) — there is no `reviewing-github-actions-code` counterpart yet, so it isn't shared the way `postgres-idioms.md`/`fsharp-idioms.md` are. Complements, doesn't duplicate, `writing-terraform-code`'s coverage of the Terraform *content* itself — this file owns the *workflow* wrapping around a `terraform plan`/`apply` step, not the HCL.

## ⚠ Read this before writing Stage 14's Azure auth step

`docs/build-plan.md` (Stage 14b) and `docs/decisions.md` (2026-07-30) already commit to GitHub Actions + OIDC, not a long-lived secret. Concretely: `azure/login@v2` exchanges a GitHub-issued OIDC token for an Azure access token via a federated identity credential on a Microsoft Entra application (or user-assigned managed identity) — no `AZURE_CREDENTIALS` client-secret JSON blob, ever ([Microsoft Learn: Use the Azure Login action with OpenID Connect](https://learn.microsoft.com/azure/developer/github/connect-from-azure-openid-connect)). Two things are easy to get wrong the first time:

- The job (or workflow) needs `permissions: id-token: write` explicitly — without it, GitHub never issues the OIDC token and `azure/login` fails opaquely. This is a real gotcha, not boilerplate to skip.
- Only `client-id`, `tenant-id`, and `subscription-id` go in `with:` (as GitHub Secrets, since none of the three are actually secret-shaped, but Secrets is still the convenient place to parameterize them per environment) — there is no `client-secret` input in the OIDC path at all. If a plan or generated workflow includes one, that's the tell it drifted back to the client-secret flow.

## Trigger scoping — CI vs. CD are different events

CI (Stage 14a) triggers on **both** `pull_request` and `push` to `main` — every PR gets restore/build/test before merge, and the same checks re-run after merge so `main` itself is proven green independent of any one PR's result, per `docs/build-plan.md`'s Stage 14a scope. CD (Stage 14b) triggers on `push` to `main` only — never on `pull_request`, since a forked/untrusted PR must never be able to trigger a workflow that holds Azure deployment credentials ([GitHub Actions security hardening: understand the risks of `pull_request_target` and forks](https://docs.github.com/actions/security-guides/security-hardening-for-github-actions)). Keep them as two separate workflow files (`ci.yml`, `cd.yml`) rather than one file with conditional jobs — cleaner separation of "runs on every PR and every merge" from "runs on merge to main with deploy credentials," and matches `docs/build-plan.md`'s explicit "CI first, CD only once CI is green" split into Stage 14a/14b.

## Job/step structure for the .NET/F# solution

A CI job is a flat sequence, not fan-out: checkout → `actions/setup-dotnet` (pin the SDK version) → `dotnet restore` → `dotnet build --no-restore` → `dotnet test --no-build`. The `--no-restore`/`--no-build` flags aren't cosmetic — they make each step fail on *its own* concern (a restore-time feed problem vs. a compile error vs. a test failure) instead of silently re-running earlier work and muddying which step actually broke.

For NuGet caching, `actions/setup-dotnet` has a built-in `cache: true` input keyed on the lockfile ([`actions/setup-dotnet` README, "Caching NuGet packages"](https://github.com/actions/setup-dotnet)) — requires `RestorePackagesWithLockFile` in the `.fsproj`/`Directory.Build.props` so a `packages.lock.json` exists to key on. **This isn't a soft fallback**: with `cache: true` set and no lock file present, the action fails the step outright rather than silently skipping the cache — so every restored project needs `RestorePackagesWithLockFile` and a committed `packages.lock.json`, not just some of them, or CI breaks on day one. Decided (`docs/decisions.md`, 2026-07-30): add the lock files as part of Stage 14a — reproducible restores (pinned exact resolved versions) are worth it on their own in a project this correctness-focused, and the `setup-dotnet` cache comes with it for free. Don't reach for the general-purpose `actions/cache` action to hand-roll what `setup-dotnet` already does for the common case.

## Terraform-in-CI: plan-then-apply, remote state is a hard prerequisite

`terraform apply` in a workflow requires the remote state backend Stage 12 sets up — a runner's filesystem is thrown away after the job, so a local `terraform.tfstate` would vanish and every run would think it's creating everything from scratch, fighting real Azure resources it no longer remembers owning ([Microsoft Learn: Deploy to Azure infrastructure with GitHub Actions, Prerequisites](https://learn.microsoft.com/devops/deliver/iac-github-actions#prerequisites)). Structure Stage 14's CD workflow the same way Microsoft's own reference architecture does: a `plan` step whose output is visible before anything is applied, and a separate `apply` step gated on that plan — never a bare `terraform apply -auto-approve` with no preceding plan review, even on the CD path where a human isn't watching in real time ([Microsoft Learn: Deploy to Azure infrastructure with GitHub Actions, Deploy with GitHub Actions](https://learn.microsoft.com/devops/deliver/iac-github-actions#deploy-with-github-actions)). Add a `concurrency:` group keyed on the environment/workflow name so two CD runs (e.g. two merges close together) can't `apply` against the same state concurrently — Terraform's state locking will reject the second run, but failing fast via `concurrency:` is cheaper than watching a run fail mid-apply.

## Secrets and environment wiring

Prefer GitHub **environment** secrets/variables (`environments: production`) over repository-wide secrets for anything deploy-related — an environment can carry protection rules (required reviewer, restricted to `main`) that a repository secret can't, and the federated credential on the Azure side can itself be scoped to `Entity Type: Environment` so the OIDC trust only resolves for jobs targeting that environment ([Microsoft Learn: Deploy to Azure infrastructure with GitHub Actions, Prerequisites](https://learn.microsoft.com/devops/deliver/iac-github-actions#prerequisites)). This is a natural fit for Summa's single-environment-for-now setup, but names the mechanism to reach for once a second (staging) environment shows up, rather than inventing branch-name-based conditionals.

## Anti-patterns to flag

- `AZURE_CREDENTIALS` / a client-secret JSON blob passed to `azure/login` instead of OIDC — the thing this repo's decision log explicitly rejected.
- Missing `permissions: id-token: write` on an OIDC job — fails at the login step, not obviously pointing at the missing permission.
- CD triggered on `pull_request` (or `pull_request_target` without exhaustive review) — hands deploy credentials to anything that can open a PR.
- `terraform apply -auto-approve` with no preceding `plan` step or artifact to review.
- Local/default Terraform state in a CI workflow — every run starts from a blank slate and fights real resources.
- Two CD runs able to race against the same Terraform state with no `concurrency:` guard.
- Skipping `dotnet test` on the CD path "since CI already ran it" — CD should still fail closed if it somehow runs on a commit CI never validated (e.g. a manually re-run workflow), not assume CI's result is still valid.
- A long-lived Azure credential (client secret, publish profile) committed to the repo or pasted directly into workflow YAML instead of a GitHub secret — even non-OIDC actions in the same workflow (if any) should pull from `secrets.*`, never inline.
