# 0011. Continuous integration now, continuous deployment later

- **Status:** Accepted
- **Date:** 2026-11-09
- **Notion decision:** None

## Context

CLAUDE.md says five checks must pass before a PR (build, test, fmt, clippy, and the `.sqlx/` query-data check), but nothing enforced it. Agents write the code (D29) and can skip a step, most often regenerating `.sqlx/` after a query change. A commit that breaks `main` breaks every branch made from it, and the failure shows up in someone else's PR.

Summa's numbers are financial numbers, so how its code changes matters on its own. Each change should be tested before it merges, with a record that it was, and the people who made the change should not be able to skip that step.

D14 keeps everything local until Go live, and D17 puts no infrastructure ahead of the milestone that needs it. `ledger` is a library, so there is nothing to deploy yet.

## Decision

**GitHub Actions runs the five checks on every PR and every push to `main`, and a ruleset on `main` refuses a merge until all five pass. Continuous deployment waits for Go live.** This does not conflict with D14 or D17: CI hosts nothing, keeps nothing running between runs and costs nothing on a public repository.

- **Five jobs, one per check,** named `fmt`, `clippy`, `build`, `test` and `sqlx-prepare`. Each is its own required check, so a red check names what failed.
- **The ruleset is the gate.** It requires a PR, requires all five checks (bound to GitHub Actions, so no other app can satisfy them), requires the branch to be up to date with `main` before it merges, blocks force pushes and deletion, and has no bypass, admins included. An admin can still edit or remove the ruleset itself, but that is a visible settings change, not a button next to Merge. Its definition is in `.github/rulesets/main.json`. There is no merge queue.
- **The database comes from `docker-compose.yml`.** CI runs `docker compose up -d --wait db`, so the PostgreSQL image is defined once and local and CI cannot drift apart. `test` needs only the server, because `#[sqlx::test]` migrates its own databases. `sqlx-prepare` also applies the migrations, because `cargo sqlx prepare --check` describes every query against the live schema.
- **Everything that could change the result is pinned.** Rust is `1.94.1` in `rust-toolchain.toml`, PostgreSQL is `postgres:18.6-alpine`, and CI builds with `--locked`. (`cargo sqlx prepare` runs one internal `cargo metadata` call without it, so `--locked` covers four of the five jobs completely.) Moving any of them forward is a deliberate PR, where new lints or behavior get dealt with on purpose.
- **Supply chain.** Every action is pinned to a full commit SHA, with its version in a comment. The only third-party action is `Swatinem/rust-cache`, which saves minutes per job. The workflow token is `contents: read`. Triggers are `pull_request` and `push` to `main`, never `pull_request_target`.

## Alternatives considered

- **An unpinned `stable` toolchain:** rejected. Every Rust release adds lints, and `-D warnings` turns them into failures on code nobody touched, while local and CI disagree.
- **A GitHub service container for PostgreSQL:** rejected. It is the conventional choice, but it names the image a second time, so local and CI could drift.
- **A merge queue:** rejected. Requiring the branch to be up to date closes the same gap, where two PRs pass separately and break `main` together. A queue pays off only when many PRs merge a day.
- **Letting admins bypass the ruleset:** rejected. A control its owner can switch off is weak evidence that changes are tested.
- **Actions pinned by tag:** rejected. Tags can be moved, so a compromised action would reach every run. A commit SHA cannot be moved.
- **A single job running all five checks:** rejected. One required check would hide which of the five failed, and the checks would run one after another.
- **Required reviews:** deferred. There is one developer, so there is no second person to approve.

## Consequences

- **Upgrading Rust or PostgreSQL is its own PR.** A process for doing it regularly is still to be decided.
- **Renaming a job means updating the ruleset.** A required check that no job reports blocks every merge.
- **The ruleset is applied by hand.** Editing `.github/rulesets/main.json` changes nothing until it is applied to the repository with `gh api`.
- **`docker-compose.yml` must keep a `db` service,** because CI starts it by that name. The observability stack (SUM-12) goes in as separate services.
- **Deployment is still to come.** When Go live creates an environment, a deployment workflow can reuse these checks as its gate.
