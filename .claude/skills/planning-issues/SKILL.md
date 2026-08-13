---
name: planning-issues
description: This skill should be used by tech-lead once a Linear Project is feasible and needs to be broken into Issues — for example "break this into issues", "plan the work for this Project", or automatically after assessing-feasibility returns a Feasible verdict. For infra-only work with no product-manager handoff, first invokes drafting-linear-projects to originate the Project. Discovers available writing-*-code skills live and creates exactly one Issue per skill invocation, per the recorded Issue-granularity decision. Always shows the full set of draft Issues and waits for explicit owner go-ahead before writing any of them to Linear.
---

## Purpose

Turn a feasible Linear Project into concrete, buildable Issues — one per `writing-*-code` skill invocation — always shown as a draft set and written only after explicit owner approval.

## Process

1. Confirm the Project has passed `assessing-feasibility`. For infra-only work with no `product-manager` handoff and no existing Project, invoke `drafting-linear-projects` first to originate one — reuse that skill's draft-and-approve flow rather than a separate mechanism.
2. Discover the available `writing-*-code` skills live via glob (`.claude/skills/writing-*-code/SKILL.md`) — never a hardcoded list; this set grows as the repo does.
3. Break the Project's scope into discrete units of work, each mapped to exactly one `writing-*-code` skill invocation — the recorded 2026-08-12 Issue-granularity decision (one Issue per skill invocation, not one per Project or one per file).
4. Draft each Issue: title, description, which `writing-*-code` skill it maps to, and team. Show the complete set of draft Issues together and wait for the owner's explicit go-ahead before calling any Linear write tool.
5. After approval, create each Issue via `mcp__linear__save_issue` at status `Todo` — Linear's built-in category, matching the pipeline-wide state-machine convention (Todo → In Progress → In Review → Done) even though nothing downstream advances it yet.
6. Report back the created Issues (titles and links) to the owner. Moving an Issue through `In Progress`/`In Review`/`Done`, and assigning it to a specific engineer agent, is out of scope here — that's `advancing-the-pipeline`/`implementing-a-linear-issue`, not yet built.

## Additional resources

### Related skills

- **`assessing-feasibility`** — must sign off on the Project before this skill runs, unless this skill is originating an infra-only Project itself.
- **`drafting-linear-projects`** — invoked from step 1 to originate a Project for infra-only work with no upstream `product-manager` handoff.
- **`recording-decisions`** — invoke if the breakdown itself surfaces a genuine new decision.
