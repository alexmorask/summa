# Summa

Summa is a billing and payments platform built on a double-entry ledger. Every financial event is a balanced transaction in an append-only log, and everything else (balances, invoices, earned revenue, tax owed) is computed from that log.

v1 bills for an AI company with a subscription product and an API product. The AI company is the reference customer, not part of the core: the ledger knows nothing about which business it serves (Notion D6). Business-specific logic never goes in `crates/ledger`. See the [Product Brief](https://app.notion.com/p/3eb0626c6d1c81d8bf12dd2e647dc5a3) and [Roadmap](https://app.notion.com/p/3eb0626c6d1c815ea234e0004b31e263).

## Domain rules

These hold everywhere in the codebase. Breaking one is a bug, not a trade-off.

- **Entries balance.** A transaction has two or more entries, and total debits equal total credits. Unbalanced transactions are rejected, never fixed up.
- **Append-only.** Ledger records are never edited or deleted. Mistakes are corrected with a reversing transaction linked to the original.
- **Whole cents.** Money is an integer number of cents with an explicit currency (USD only in v1). No floats, no fractions of a cent. Entry amounts are always positive; direction comes from the debit or credit side.
- **Time is an input.** Code never reads the system clock directly. The current time is always passed in, so the simulated clock can drive renewals, revenue recognition and expiry (ADR 0003). Enforced by `clippy.toml` (`disallowed-methods`).

The full rule set for the ledger core is in the [Ledger Core Spec](https://app.notion.com/p/3eb0626c6d1c81569e03c56bacad5482). Account definitions are in the [Chart of Accounts](https://app.notion.com/p/3eb0626c6d1c8144a930f4e3e6248d76).

## Layout

A Cargo workspace laid out as a modular monolith (ADR 0002): one app, with each module in its own crate under `crates/`.

- `crates/ledger`: the ledger core.

Add a crate only when a milestone needs it.

Every crate inherits the workspace lints (`[lints] workspace = true`); unsafe code is forbidden.

## Commands

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

All four must pass before a PR.

## Testing

- Every domain rule has a test, including the rejection case.
- Rules that must always hold (e.g. every transaction balances) also get property-based tests with `proptest`.
- Tests use a fixed, passed-in time, never the system clock.

## Conventions

- **Commits and PR titles:** Conventional Commits with the Linear ticket ID at the end, e.g. `feat(ledger): money and accounts (SUM-6)`.
- **PRs:** fill in `.github/pull_request_template.md`, including the docs check.
- **Tickets:** tracked in Linear (team Summa, `SUM-`).

## Decisions

- **Technical decisions** (how it's built) are architecture decision records in `docs/adr/`, numbered `NNNN-short-title.md`. Each references the Notion decision it comes from, if any. Don't edit an accepted ADR's decision; write a new one that supersedes it.
- **Product decisions** (what and why) are in the [Product Decision Log](https://app.notion.com/p/3eb0626c6d1c81e79d5ccbf1acb6d874) in Notion, as D1, D2 and so on.
