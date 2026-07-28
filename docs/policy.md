# Policy

_Status: stub — deep dive pending._

**Purpose:** the composable pricing algebra. Answers *how much* to charge.

**Idea so far:** instead of a fixed Stripe-style "Plan" (price + interval
bundle), model pricing as small composable primitives — flat, tiered, volume,
per-unit, and time-boxed discounts/overrides — that combine like building
blocks. New pricing shapes become new combinators, not new API surface.
Published policies are immutable and versioned once live.

**Relationship to Performance Obligation:** siblings. Policy = *how much*;
Performance Obligation = *how that amount becomes earned over time* (see
`ledger.md`).

**To decide in the deep dive:** the exact primitives, how they compose,
versioning/immutability rules, how a Policy is attached to a customer (see
`agreement.md`).
