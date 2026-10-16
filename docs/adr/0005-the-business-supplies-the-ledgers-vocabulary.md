# 0005. The business supplies the ledger's vocabulary

- **Status:** Accepted
- **Date:** 2026-10-16
- **Notion decision:** [D6. One reference business per version; the core stays general](https://app.notion.com/p/3eb0626c6d1c81e79d5ccbf1acb6d874)
- **Corrects:** one consequence of [ADR 0004](0004-currency-as-a-type-parameter.md) (see the end)

## Context

The ledger knows nothing about which business it serves (D6). Two parts of a transaction still name business things:

- **Tags on entries,** like customer 42 or subscription 9. The Ledger Core Spec requires tag kinds to come from a fixed set defined in code, so a misspelled kind is caught. But `customer` and `subscription` are the AI company's words, and the v2 marketplace will need others, like `seller` and `buyer`.
- **Accounts.** Every entry's account must exist in the chart of accounts, and the chart belongs to the business.

If the ledger defined either one, every business would have to edit the ledger to add its own words.

## Decision

**The ledger defines what a tag and an account are. The business supplies which ones exist.**

- **Tag kinds come from the business and are checked at compile time.** The ledger defines a `Tag` trait (`fn kind(&self) -> &'static str`) and is generic over it. The business implements it on its own enum: one variant per kind, each carrying a typed value, like `Customer(CustomerId)`. A misspelled kind doesn't compile, and a subscription ID can't be passed where a customer ID belongs.
- **The chart is passed in and checked at runtime.** The caller gives the ledger a `Chart` when building a transaction, the same way it gives `now` (ADR 0003). Entries hold an `AccountNumber`, which is looked up in the chart; an entry whose number isn't there is rejected.
- **An account's identity is its number.** The chart holds the only copy of each account and rejects two accounts with the same number. Renaming an account changes the chart's copy and no entry.

## Alternatives considered

- **Tag kinds defined in the ledger:** rejected. It puts the business's words in the core, and every milestone would edit the ledger crate to add a kind.
- **Tag kinds as strings registered at runtime:** rejected. A misspelling becomes a runtime error, or quietly creates a new kind.
- **A business-supplied list of kinds with string values:** rejected. Values are untyped, so `Customer("sub_9")` compiles.
- **Entries holding a copy of the whole `Account`:** rejected. A copy can disagree with the chart (number 1000 typed as a liability), which needs a check the number lookup makes unnecessary, and identity would depend on every field, so a rename would look like a different account.
- **The business defining its accounts as a type:** rejected. The chart has to be data from M2, where it's checked against stored entries.

## Consequences

- **The tag type is a second type parameter that spreads like currency does:** entries, transactions and balance queries take `<C, T>`.
- **Tag kinds stored as strings are parsed at the boundary** into the business's tag type in M2, the same way currency codes are (ADR 0004). `Tag::kind` gives the string when saving.
- **The playground supplies its own tags and chart** in the ledger's tests (`crates/ledger/tests/common/`), so ledger source has no business words.
- **Code that needs an entry's account details,** like its type or normal side, goes through the chart, which is passed in anyway.

## Correction to ADR 0004

ADR 0004's Consequences say the number of decimal places a currency uses "is added with the first such currency". It's added now instead, as `Currency::DECIMAL_PLACES` (2 for US dollars), because rejection messages show amounts like `21.60 USD`, and formatting needs it. ADR 0004's decision is unchanged.
