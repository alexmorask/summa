---
name: writing-fsharp-code
description: This skill should be used when the user asks to write, implement, or draft F# code in the Summa repository — for example "write the Transaction type", "add the decide function", "draft the event store port", "implement LIN-123", or any request to produce new or modified .fs/.fsi source. Covers drafting an implementation plan, writing idiomatic F#, and explaining unfamiliar constructs afterward, per this repo's working agreement.
---

## Purpose

Implement F# code for the Summa ledger idiomatically, while honoring this repo's working agreement in `.claude/CLAUDE.md`: the owner must understand every line, so writing code is a two-phase, approval-gated process, not a one-shot action.

## Process

1. Read `.claude/CLAUDE.md` in full, and the relevant sections of `docs/ledger.md`, `docs/architecture.md`, and `docs/decisions.md` for the task at hand. Scope is whatever the owner asked for directly, or whatever a Linear Issue handed off via `implementing-linear-issues` specifies — don't expand beyond it into adjacent work the owner hasn't asked for.

2. Draft an implementation plan before writing anything — file by file, what changes and why. Use Plan Mode (`EnterPlanMode` / `ExitPlanMode`) so the plan is presented for explicit approval before any file is written; this is not optional politeness, it is the literal working agreement ("Explain before writing... wait for approval before writing code"). Keep the plan to only what the current task needs — no interfaces, layers, or helpers "for later" (the event store port is the one recorded exception in `docs/decisions.md`).

3. While planning and writing, consult `../references/fsharp-idioms.md` for formatting, component design, domain modeling, performance, error-handling, and testing idioms, each cited to its source. In particular, evaluate advanced patterns (active patterns, computation expressions, SRTP) on genuine merit rather than avoiding them by default — see that file's "Advanced patterns" section. If one is the right fit, recommend it explicitly as part of the plan and explain what it buys and costs; this repo's prime directive is a reason to teach the idiom deliberately, not a reason to reach for the plainest form on reflex.

4. Hold these Summa-specific invariants as non-negotiable, overriding generic F# advice when they conflict (full detail in `.claude/CLAUDE.md`):
   - `Result` over exceptions for domain errors; exceptions stay reserved for genuinely exceptional infrastructure failures.
   - The domain core (`Summa.Ledger.Domain`) is pure — no I/O, no clock, no `Guid.NewGuid()` inside decision functions.
   - Money is `int64` minor units — never float, never decimal.
   - `.fsproj` `<Compile>` order is significant and must match actual dependency direction.
   - Standard accounting vocabulary only (Performance Obligation, Deferred Revenue, Accounts Receivable) — no invented synonyms.
   - Every `Transaction` balances, enforced by a smart constructor returning `Result`, not by caller discipline.
   - The ledger is append-only; corrections are reversing entries linked via `CausationId`, never edits.

5. After the plan is approved, write the code.

6. After writing, explain every non-obvious F#/.NET construct used, in the chat reply — never in code comments. This step is mandatory, not optional polish; it is the entire point of this skill's use in this repo. Include the reasoning for any advanced pattern used, per step 3.

7. If drafting the plan (step 2) or writing the code (step 5) surfaced a genuinely new decision — a library or package choice, a data shape not already specified in the docs, resolving one of the docs' open questions — invoke the `recording-decisions` skill before considering the task done. Do not skip this because it feels like overhead; it is the mechanism that keeps `docs/decisions.md` trustworthy.

## Additional resources

### Reference files

- **`../references/fsharp-idioms.md`** — formatting, component design, domain modeling, performance, error handling, testing, advanced-pattern evaluation, and a list of F# anti-patterns to avoid introducing.

### Related skills

- **`recording-decisions`** — invoked from step 7 whenever the plan or the implementation involves a decision not already recorded.
