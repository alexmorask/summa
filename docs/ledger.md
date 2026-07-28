# Ledger

_Status: deep-dived 2026-07-23. The foundation everything else sits on._
_Unfamiliar term? See `glossary.md`._

## What it is

The append-only record of money facts — the single source of truth. Nothing
else in the platform is authoritative; everything else is computed from this.

## Foundation: double-entry bookkeeping

Every financial event is recorded as a *transaction* made of two or more
*entries* (debits and credits) that must net to zero. Because each transaction
balances locally, the global invariant "every account across the whole ledger
sums to zero" is guaranteed automatically — you can replay the entire history
and assert it balances, with no global lock. This is the core "correctness
under failure" showcase.

## Append-only & corrections

Entries are never edited or deleted. A mistake is fixed by posting a *reversing
entry* (its inverse) plus the correct entry, leaving a full audit trail.

Supporting rules:
- Money stored as integer minor units (cents), never floats.
- Every transaction carries an idempotency key so a retried command can't
  double-post.

## The three moments (kept separate)

Naive systems collapse these into one; a correct ledger keeps them distinct:

1. **Billing** — assert the customer owes us (a receivable appears).
2. **Collection** — cash actually arrives.
3. **Recognition** — we're permitted to count it as earned revenue.

They happen at different times and in different orders (prepaid → recognize
after collecting; arrears → recognize before collecting; usage → recognize as
consumed). A ledger that can't tell them apart can't be correct.

## Core accounts (starting chart of accounts)

| Account | Type | Meaning |
|---|---|---|
| Accounts Receivable | Asset | Billed but not yet paid |
| Cash | Asset | Collected |
| Deferred Revenue | Liability | Paid/billed but not yet earned; we still owe service |
| Revenue | Income | Earned |
| Tax Payable | Liability | Tax charged to customers, owed to the tax authority (recorded; calc logic deferred) |
| Processing Fee Expense | Expense | The payment processor's cut — a cost of doing business |

_(In a "billed-in-arrears" processor model, fees accumulate in a **Fees Payable**
liability instead of being netted per payout. Fee handling is done for real in
the Settlement phase — see `settlement.md`.)_

**Scheduled-settlement liabilities.** Tax Payable (and Fees Payable, if used)
accumulate over a period and are discharged on a recurring remittance/settlement
date, not immediately — every taxable sale credits Tax Payable; you clear it on
the authority's filing schedule (debit Tax Payable, credit Cash). The moment a
liability is incurred is decoupled from when cash settles it — the same
billing-vs-collection split as the three moments. "What do we owe right now?" is
just a balance query.

**Ledger accounts are internal bookkeeping categories, not links to banks.**
"Cash" here is the accounting representation of money held, not a live connection
to a bank account. The system does connect to real institutions, but through a
separate settlement layer; the ledger's accounts are the internal mirror that
layer keeps honest. Mapping internal accounts to real external accounts (bank,
processor) and proving they agree is the **Settlement & reconciliation** context's
job (see `settlement.md`), not the ledger's.

**Configurability.** The chart of accounts is a *defined domain model* — a fixed
set of account *types* (in F#, essentially a discriminated union), extensible by
us at design time, not free-form end-user configuration (that's general-ledger
software like QuickBooks). Account *instances* can be parameterized per entity
(per customer, per currency, per jurisdiction). Posting rules ("this business
event → these balanced entries") are pure, tested functions — the FP "decide"
step — not runtime config.

_Open: credits/promotions, write-offs, multi-currency handling. See Open Questions._

## Worked example — $120 annual plan, prepaid in January

- **Invoice issued:** debit Accounts Receivable $120, credit Deferred Revenue $120
- **Payment collected:** debit Cash $120, credit Accounts Receivable $120
- **Each month ×12:** debit Deferred Revenue $10, credit Revenue $10

After 12 months Deferred Revenue is $0 and Revenue is $120. Cash arrived in
month 0 but revenue landed over 12 months — that gap is exactly what revenue
recognition exists to model.

### Detail: how a customer payment posts

The customer was invoiced $120 (debit Accounts Receivable, credit Deferred
Revenue). When their $120 payment arrives:

- **debit Cash $120** — Cash is an Asset, home side is the left, so a debit
  grows it. Cash: $0 → $120.
- **credit Accounts Receivable $120** — Accounts Receivable is also an Asset, so
  a credit (its *non*-home side) shrinks it. Receivable: $120 → $0.

The payment is a swap of one Asset (a receivable — a promise to pay) for another
(cash). Two things worth burning in:

1. **Revenue and Deferred Revenue don't move at all.** Getting paid is not
   earning — earning is the recognition schedule's job, later.
2. **Same word, opposite effect:** this $120 *credit* shrinks an Asset
   (Receivable), whereas the $120 *credit* at invoicing *grew* a Liability
   (Deferred Revenue). Debit/credit only mean "left/right"; whether that's up or
   down depends on the account's home side. See `glossary.md` → Account types.

## Performance Obligation

Created at the moment of billing as a concrete instance ("$120, recognize $10
on the 1st of each month, Jan–Dec"). Distinct from pricing:

- **Policy** answers *how much* → $120.
- **Performance Obligation** answers *how that amount becomes earned over time*
  → $10/month × 12.

The *rule* ("annual service → straight-line over 12 months") is a reusable
template living with the product/Policy; the *schedule* with real dates for one
customer's one charge is the instance stamped out at billing. Template up top,
instance per charge.

## Recognition mechanism (hybrid)

- The full schedule of future recognitions is fixed the instant the obligation
  is created — a known, deterministic plan.
- A background job walks the schedule and posts each recognition entry (debit
  Deferred Revenue, credit Revenue) into the ledger as its period comes due.
- Each entry has a deterministic id (obligation id + period), so the job is
  **idempotent**: safe to run twice, late, or after a crash — it only fills in
  what's missing, never double-posts.
- "How much have we earned so far?" is also answerable instantly by counting the
  schedule's past-due entries; this always agrees with the posted ledger because
  both come from the same fixed schedule.
- The not-yet-due portion of the schedule is a *plan, not a fact*, so it stays
  **outside** the ledger until each entry's date arrives — the ledger only holds
  things that have actually happened.

## Transaction — the write model

A **Transaction** is one *balanced financial event*: the atomic unit appended to
the log. Grain rule: **one Transaction = one business event** (a moment money
moves or an obligation changes), *not* one line-item. The components of a bill
(service, tax, fee) are **entries**, not separate transactions — and each lands
in whichever transaction matches the moment it becomes known (service + tax at
invoicing; the processor fee at settlement).

```fsharp
type AccountId = AccountId of string      // "cash", "ar:cust_123", "deferred_revenue"
type Money     = int64                     // minor units (cents); always positive
type Direction = Debit | Credit

type Entry =
    { Account   : AccountId
      Direction : Direction
      Amount    : Money }

type Transaction =
    { Id             : Guid
      OccurredAt     : DateTimeOffset       // effective (business) date, separate from record order
      Description    : string
      CorrelationId  : string               // the business thing this relates to — shared across related txns
      CausationId    : string               // the specific command/event that caused THIS txn
      IdempotencyKey : string
      Entries        : Entry list }         // 2+, debits must equal credits
```

**Self-validating.** Construction goes through a smart constructor returning
`Result`, so an unbalanced Transaction is unrepresentable (illegal states
unrepresentable):

```fsharp
type LedgerError =
    | TooFewEntries
    | NonPositiveAmount
    | Unbalanced of debits: int64 * credits: int64

module Transaction =
    let create id occurredAt description correlation causation idem (entries: Entry list) =
        let total dir =
            entries |> List.sumBy (fun e -> if e.Direction = dir then e.Amount else 0L)
        let debits, credits = total Debit, total Credit
        if   List.length entries < 2                          then Error TooFewEntries
        elif entries |> List.exists (fun e -> e.Amount <= 0L) then Error NonPositiveAmount
        elif debits <> credits                                then Error (Unbalanced (debits, credits))
        else Ok { Id = id; OccurredAt = occurredAt; Description = description
                  CorrelationId = correlation; CausationId = causation
                  IdempotencyKey = idem; Entries = entries }
```

**Correlation vs causation** (standard event-driven pattern):
- `CorrelationId` groups every transaction in one workflow (a purchase); shared
  across all of them.
- `CausationId` points at the immediate cause of *this* transaction; differs per
  txn.

"Show me the whole story of a purchase" is then a query by `CorrelationId`, not a
join across mutable records — the same id later powers the audit trail and the
"explain my bill" assistant.

**Example — one purchase, two transactions, linked by CorrelationId:**

```
// invoice — service + tax known now
CorrelationId "charge:cust_123:2026-01"  ·  CausationId "AgreementActivated:cust_123"
  Debit  ar:cust_123        13200
  Credit deferred_revenue   12000   // service
  Credit tax_payable:CA      1200   // tax          (13200 = 12000 + 1200 ✓)

// settlement — fee known now — SAME CorrelationId
CorrelationId "charge:cust_123:2026-01"  ·  CausationId "PaymentSettled:pi_abc"
  Debit  cash                   12850
  Debit  processing_fee_expense   350
  Credit ar:cust_123            13200               (12850 + 350 = 13200 ✓)
```

**Grain rule:** things known in the same atomic business event → entries in one
transaction; things known at different moments → separate transactions linked by
the same `CorrelationId`. (Tax computed asynchronously later would be its own
transaction under the same correlation id.) Corrections/reversals are likewise
new transactions with inverse entries, pointing back via `CausationId` — never
edits.

## Open questions (ledger-specific)

- Full chart of accounts — add tax, credits/promotions, write-offs?
- Remaining timing cases to walk through: pay-monthly (arrears),
  pay-after-delivery.
- Aggregate / consistency boundary — per-account, per-customer, or
  per-transaction optimistic concurrency?
- Multi-currency — single currency to start, or currency as a dimension?
- Does Performance Obligation split into its own context/file?
