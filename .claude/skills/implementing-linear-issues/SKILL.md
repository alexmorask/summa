---
name: implementing-linear-issues
description: This skill should be used by software-engineer or infrastructure-engineer when a Linear Issue at Todo needs to be picked up and implemented — for example "implement LIN-XXX", "pick up the next issue", or automatically whenever an engineer agent is invoked with an Issue in hand. Reads the Issue, moves it through Linear's real status names (discovered live via list_issue_statuses, never hardcoded), invokes the Issue's mapped writing-* skill (that skill's own Plan Mode gate is where the owner approves the design — this skill adds no second gate), runs the relevant build/test commands, invokes updating-readme and updating-claude-md before committing, then opens a PR by replicating commit-push-pr's steps directly via Bash (that plugin command isn't invocable by a subagent), and moves the Issue to the in-review status.
---

## Purpose

Take a Linear Issue that `tech-lead` has already scoped and vetted, and turn it into an opened PR, using this repo's existing conventions and skills at every step rather than reinventing any of them.

## Process

1. Read the Issue (`mcp__linear__get_issue`, or `mcp__linear__list_issues` if the owner says "pick up the next one" rather than naming an ID). Confirm it names which `writing-*` skill it maps to, per `planning-linear-issues`' drafting contract. If it doesn't, stop and ask the owner rather than guessing.
2. Discover the Issue's team's real status names via `mcp__linear__list_issue_statuses` — never hardcode "In Progress" as a literal string. Move the Issue to the in-progress category via `mcp__linear__save_issue`'s `state` field.
3. Create a feature branch off `main` — plain kebab-case, no prefix, matching this repo's existing branch-naming convention.
4. Invoke the Issue's mapped `writing-*` skill to implement it. That skill's own `EnterPlanMode`/`ExitPlanMode` gate is where the owner approves the actual design before any file is written — this skill does not add a second gate around it.
5. Run the build/test command(s) matching the Issue's mapped skill, and confirm they pass before proceeding — never assume `dotnet build`/`dotnet test` applies to every Issue:
   - `writing-fsharp` → `dotnet build`, `dotnet test`.
   - `writing-sql` → `docker compose up -d` and `./db/migrate.sh` first, so the change can actually be exercised locally.
   - `writing-terraform` → `terraform validate` only. Never `terraform apply` — Terraform is applied by CD on merge to `main`, not by this skill.
   - `writing-dockerfiles` → `docker compose up --build`, to verify the image(s) actually build and run.
   - `writing-github-actions-workflows` → no local equivalent exists in this repo; the real `ci.yml` run against the opened PR is the test — note that in the PR's `## Test plan` rather than leaving it blank.
6. Invoke `updating-readme` and `updating-claude-md` before committing — both skills' own trigger is "right before a commit," and nothing else in this pipeline currently chains into them.
7. Commit (a single-line, imperative-mood message matching this repo's existing style), push the branch, and open a PR via `gh pr create` with a `## Summary`/`## Test plan` body, matching this repo's real PR-body convention. This replicates the `commit-push-pr` plugin command's own steps directly via `Bash`, since that command is a slash command with no accompanying Skill file — a subagent has no mechanism to invoke it directly.
8. Move the Issue to the in-review category via `mcp__linear__save_issue`, and report the PR link back to the owner. Stop there — handing the PR to `tech-lead`'s `reviewing-engineer-code` is a separate, manually triggered step, not something this skill chains into automatically, consistent with this pipeline's interactive-only orchestration.

## Additional resources

### Related skills

- **`writing-fsharp`** / **`writing-sql`** / **`writing-terraform`** / **`writing-dockerfiles`** / **`writing-github-actions-workflows`** — whichever the Issue names; invoked from step 4 to do the actual implementation, including its own approval gate.
- **`updating-readme`** / **`updating-claude-md`** — both invoked from step 6, right before the commit.
- **`recording-decisions`** — invoked transitively if the mapped `writing-*` skill's own process surfaces a genuine new decision while implementing.
