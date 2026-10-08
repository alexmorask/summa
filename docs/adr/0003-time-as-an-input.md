# 0003. Time is an input

- **Status:** Accepted
- **Date:** 2026-10-07
- **Notion decision:** [D12. Demo runs on simulated providers and a controllable clock](https://app.notion.com/p/3eb0626c6d1c81e79d5ccbf1acb6d874)

## Context

Much of billing is driven by time passing: renewals, month-end invoices, revenue moving from deferred to earned, failed-payment retries and expiring credits. Two needs follow:

- **The demo needs a controllable clock.** A visitor can't wait a month for a renewal. They need to fast-forward and watch it happen (D12).
- **Tests need to be deterministic.** A rule like "effective time can't be in the future" can't be tested reliably if the code asks the machine what time it is.

If any code reads the system clock, the simulated clock stops being the whole truth: part of the system runs on real time and part on demo time, and results depend on when they ran.

## Decision

**Code never reads the system clock directly. The current time is always passed in.**

- **Pure code takes `now` as an argument.** A rule that needs the current time, like rejecting effective times in the future, receives it as a parameter, the same as any other input.
- **The app's edge owns the clock.** The application shell gets the current time from a clock it is given: the real clock in production, a simulated one in the demo and a fixed one in tests. That real clock implementation is the only place that reads the system time.
- **All times are UTC.** Otherwise "September 30" means different moments to different callers (Ledger Core design doc, Failure 8).
- **Scope:** this covers business time, the time that decides what happens or which period something counts toward. Measuring how long an operation took, for metrics and traces, isn't business time and may use the system's timers.

## Open question

**Where the ledger's recorded time comes from.** The Ledger Core design doc (Failure 8) says recorded time must come from one clock, the database's, never from the machines running the code. In the demo, the database's clock is real time while effective times come from the simulated clock, so a fast-forwarded transaction would be recorded before it took effect. This is decided in M2 (SUM-9), when transactions are first saved and recorded time is first stamped.

## Alternatives considered

- **Read the system clock where needed, and mock it in tests:** rejected. Mocking global time is fragile in Rust, and it doesn't help the demo, which needs the running app, not just tests, to follow the simulated clock.
- **A global, settable clock:** rejected. It hides which code depends on time, and parallel tests that set it would interfere with each other.

## Consequences

- **Time dependence is visible in signatures.** Any function that takes `now` depends on time; any that doesn't, can't.
- **Tests are deterministic** and can cover month-ends, leap years and expiry without waiting or mocking.
- **The demo's fast-forward works across the whole app,** because nothing is reading real time behind its back.
- **Cost:** `now` gets threaded through call chains. A stray clock read is caught by Clippy's `disallowed-methods` (in `clippy.toml`), which fails `cargo clippy --all-targets -- -D warnings`. The real clock implementation is the one place allowed to opt out, with `#[allow(clippy::disallowed_methods)]`.
