# 0001. Rust for the backend, with Go as the fallback

- **Status:** Accepted
- **Date:** 2026-10-07
- **Notion decision:** [D16. Rust, with Go as the fallback](https://app.notion.com/p/3eb0626c6d1c81e79d5ccbf1acb6d874)

## Context

Summa needs one backend language (D15 rules out a language per service). The choice has to serve three goals at once:

- **The thesis.** Summa claims a billing platform can be built on a double-entry ledger by composing small, pure building blocks. A language that can make wrong states impossible to represent shows that claim in the code itself, not just in tests.
- **Learning.** Learning Rust and gaining functional programming experience are explicit goals of the project (D3).
- **Hiring.** Summa is a portfolio piece. The language should open doors that are otherwise closed.

The main risk is pace. The project runs on a few hours a week. AI agents write the code (D29), but every change has to be understood, and an unfamiliar language makes that slower.

## Decision

The backend is written in Rust.

**Checkpoint:** after the ledger core (M1–M3), if it took more than about twice its planned time *because understanding Rust slowed the work*, judged by me at the M3 acceptance ticket (SUM-13), reassess, with Go as the fallback.

## Alternatives considered

- **Go:** the most hireable choice for payments backends, but not functional, and its type system can't rule out as many wrong states. It's the fallback.
- **Haskell, OCaml:** strongly functional with excellent type systems, but small hiring markets and thin ecosystems for payments work.
- **Scala, Kotlin:** capable and partly functional, but tied to the JVM and less sought after for new payments backends than Rust or Go.
- **TypeScript:** already known, so it adds little learning, and its types are erased at runtime, so it can't guarantee wrong states are impossible the way Rust can.

## Consequences

- **The type system enforces ledger rules.** For example, a balanced transaction can be a type that only validation can produce, so code that receives one never has to re-check it.
- **Hiring:** Rust shops tend to require proven Rust experience, while Go shops expect engineers to learn Go quickly. A Rust project opens more new doors.
- **Slower start.** Early milestones will take longer while learning the language. The checkpoint after M3 limits how much that can cost.
- **Pure rules come first.** M1 is pure Rust with no database, so the language is learned on the domain before storage and async code arrive.
