---
name: drafting-linear-projects
description: This skill should be used when the owner wants a refined idea turned into Linear — for example "create a ticket for this", "let's put this in Linear", "sync this to Linear", or when updating an existing Project as a spec evolves. Always checks Linear for an existing matching Project first, always shows the full draft and waits for explicit go-ahead before writing, and never creates Issues underneath the Project.
---

## Purpose

Turn a refined product spec (from `refining-ideas`) into a Linear Project — the only step in the `product-manager` workflow that writes to Linear, and only after the owner explicitly approves a shown draft.

## Process

1. Query Linear (`mcp__linear__list_projects`) for an existing Project that might already represent this idea. Decide create-vs-update explicitly rather than assuming a new Project is needed.
2. If more than one Linear team exists (`mcp__linear__list_teams`), ask the owner which team the Project belongs to. Never hardcode a team or project ID.
3. Draft the Project's title and description from the refined spec: problem, why now, affected bounded context(s), open questions/risks. New Projects land in **Backlog** status, matching the pipeline-wide state-machine convention (Backlog → Planned → In Progress → Completed) even though nothing downstream advances it yet. Never create Issues under it — that's `tech-lead`'s job, not built yet.
4. Show the complete draft — title, description, status, team — and wait for the owner's explicit go-ahead before calling any Linear write tool.
5. After approval, create or update the Project via `mcp__linear__save_project` and report back the result (name and link) to the owner.

## Additional resources

### Related skills

- **`refining-ideas`** — produces the spec this skill drafts from; never invoke this skill on an unrefined idea.
