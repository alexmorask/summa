# Accounting Glossary

_Plain-English anchors. Grows as we introduce terms. Standardized terms used
throughout, anchored in plain English on first use._

| Term | Plain meaning |
|---|---|
| **Ledger** | The official, append-only record of money facts. Single source of truth. |
| **Double-entry** | Every transaction records both where money came from and where it went; the two sides always equal. |
| **Debit / Credit** | Directional entries. NOT good/bad or add/subtract — whether they increase or decrease a balance depends on the account type (see below). |
| **Transaction** | A group of entries (debits + credits) that nets to zero — the atomic thing appended to the ledger. |
| **Account** | A bucket that money moves between. |
| **Revenue Recognition** | Moving an amount from Deferred Revenue to Revenue as service is actually delivered. |
| **Performance Obligation** | The promise of service that revenue is recognized against; created at billing, carries the recognition schedule. (ASC 606 term.) |
| **Trial balance** | A check that all account balances sum to zero — proof the books balance. |
| **Idempotent** | Safe to repeat: doing it twice gives the same result as doing it once. |

## Account types (the part flagged as confusing)

**The mental model — every account has a "home side."** Debit and credit don't
mean increase/decrease or good/bad. They literally mean "left column" and "right
column" (500-year-old Latin bookkeeping labels). Accounts are split by which side
of the books they live on: Assets live on the **left**; Liabilities, Revenue, and
Equity live on the **right**. The rule that makes everything fall out: **an
account grows on its home side and shrinks on the other side.**

| Type | Plain meaning | Home side | Example account | Balance goes UP with a… |
|---|---|---|---|---|
| **Asset** | Something you own or are owed | Left | Cash, Accounts Receivable | Debit |
| **Liability** | Something you owe | Right | Deferred Revenue (you owe service) | Credit |
| **Revenue** | Value you've earned | Right | Revenue | Credit |
| **Equity** | Owners' residual stake | Right | (not used yet) | Credit |
| **Expense** | Cost of doing business | Left | (later: processing fees) | Debit |

The one rule that always holds: within any transaction, total debits = total
credits.

**Why "credit" feels backwards from everyday life:** when your bank "credits your
account," that's *their* books — to the bank, your balance is a Liability (money
they owe you), and liabilities grow with a credit. You feel your asset went up;
they're recording their liability going up. Both correct, different books.

**Same word, opposite effect:** a $120 *credit* to Accounts Receivable (Asset)
*decreases* it, while a $120 *credit* to Deferred Revenue (Liability) *increases*
it — purely because the account types differ. Think "home side," never memorize.
