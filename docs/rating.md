# Rating

_Status: stub — deep dive pending._

**Purpose:** pure functions that turn "ledger events + policy timeline for a
period" into a statement/invoice. No independent storage; everything is computed
on demand and fully replayable.

**Key idea:** an invoice is not a stored, mutable document — it's a pure
computed statement, `invoice(customer, period) = rate(usage, policyTimeline)`.
Regenerating a past invoice is just calling the function again — deterministic
and replayable. Re-rating history when pricing rules change is the signature
event-sourcing "replay" showcase.

**To decide in the deep dive:** exact function signatures, how usage folds in
(Phase 2), re-rating semantics and edge cases.
