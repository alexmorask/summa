# 0004. Currency as a type parameter

- **Status:** Accepted
- **Date:** 2026-10-14
- **Notion decision:** None

## Context

Every amount of money states its currency, and amounts in different currencies must never be added together. v1 handles only US dollars, but the Ledger Core Spec requires that adding currencies later means new rules, not reworking old data.

The ledger needs a way to make "adding dollars to euros" impossible. That can be checked when the code runs, or by the compiler before it runs.

## Decision

**The currency is part of the money type: `Money<C: Currency>`.**

- **Amounts are `Money<Usd>`, not `Money` with a currency field.** `Money<C>` holds a whole number of minor units (`i64`; cents, for US dollars) and a zero-sized `PhantomData<C>` marker, so it costs nothing at runtime.
- **Mixing currencies is a compile error.** `checked_add` takes another `Money` of the same `C`, so adding a `Money<Eur>` to a `Money<Usd>` doesn't compile.
- **Each currency is a marker type implementing the `Currency` trait,** which carries its code (`Usd::CODE == "USD"`).
- **`Currency` is not sealed.** A doc test defines a second currency to prove that mixing is rejected, and a sealed trait would stop it. USD-only is a v1 scope rule, enforced where currency strings are parsed.

## Alternatives considered

- **A `Currency` enum field with a runtime mismatch error:** rejected. Mixing currencies becomes a runtime error that only shows up when that code runs, instead of a compile error. Its advantage is that currency strings from storage or requests need no parsing into types.

## Consequences

- **Currency strings are parsed at the boundary.** Currency arriving as text (from storage or a request) is turned into the right type in one place, usually a small `Currency` enum and a `match`, starting in M2.
- **One ledger, or one set of balances, holds one currency.** A single collection can't hold `Money<Usd>` and `Money<Eur>`. Combining currencies needs a conversion rule, which doesn't exist in v1.
- **The parameter spreads to types that carry money:** entries, transactions and balance queries take `<C>`. `Account` doesn't, because an account holds no money.
- **Amounts are counted in minor units, not "cents".** `Money<C>` is generic over currency, so its API (`from_minor_units`, `minor_units()`) doesn't assume two decimal places. Currencies with a different number (the yen has none) will need the trait to state it; that's added with the first such currency.
