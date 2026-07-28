# Settlement & Reconciliation

_Status: stub — deep dive pending._

**Purpose:** where external payment provider activity enters the ledger, plus
the reconciliation job that compares internal ledger state against provider
truth and surfaces drift.

**Key ideas:** a simulated payment provider sends webhooks with duplicates,
out-of-order delivery, and retries; the system processes them idempotently and
appends settlement events to the ledger. "Is this paid?" is a derived query over
ledger entries, not a status flag mutated in place. A reconciliation job catches
drift when internal and provider views disagree. This context most directly
mirrors Alex's published writing on idempotency and reconciliation.

**To decide in the deep dive:** provider simulation design, webhook idempotency
model, what drift detection compares and how it reports.
