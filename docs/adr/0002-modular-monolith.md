# 0002. A modular monolith, one crate per module

- **Status:** Accepted
- **Date:** 2026-10-07
- **Notion decision:** [D15. One backend language plus TypeScript, as a modular monolith](https://app.notion.com/p/3eb0626c6d1c81e79d5ccbf1acb6d874)

## Context

Summa will grow into several modules: the ledger core, then catalog and subscriptions, payments, invoicing and tax, credits and usage. They need clear boundaries, because the thesis is that the platform is built by composing small pieces, and because the ledger must stay general: it knows nothing about which business it serves (D6).

The project runs on a few hours a week and stays local until Go live (D14). Separate services would mean separate deploys and network calls between modules, which breaks the guardrail of building no infrastructure until a milestone needs it (D17).

## Decision

The backend is **one Rust application with strict internal boundaries**, plus a TypeScript front end. No separate services yet.

Each module is its own crate in a Cargo workspace, under `crates/`:

- A module's public API is what its crate exports. Everything else stays private to the crate.
- A module can only use another module it lists as a dependency in its `Cargo.toml`, and Cargo rejects dependency cycles. Boundaries are checked by the compiler, not by convention.
- `crates/ledger` depends on no other Summa module. Business modules depend on the ledger, never the other way around.
- Modules call each other in-process. Ledger posting is a synchronous function call (D23).
- A crate is added when a milestone needs it, not ahead of time.

## Alternatives considered

- **Services, each in the best language for it:** rejected. A few hours a week can't sustain several languages and deploy pipelines, and splitting into services early is building infrastructure before the product needs it.
- **One crate with modules (`mod`) for boundaries:** simpler, but Rust visibility within a crate is easy to loosen and doesn't stop modules depending on each other in a cycle. Separate crates make the boundaries compiler-enforced.

## Consequences

- **Boundaries are visible and enforced.** The dependency graph between crates is the architecture diagram, and a wrong dependency fails to compile.
- **One build, one deploy, one process.** No network calls between modules, no distributed transactions, and nothing to run but the app and its database.
- **A clear story for splitting later.** Because modules already talk through public APIs, any one of them can move into its own service when scale demands it. The ledger is the likely first candidate: the design doc already recommends giving it its own database at real scale.
- **Cost:** more `Cargo.toml` files to maintain, and a change that crosses modules touches several crates.
