//! The playground chart: accounts from the reference business's chart of
//! accounts, shared by every test that needs them. They live in tests, not in
//! the ledger, because the ledger knows nothing about the business it serves
//! (D6).

use ledger::{Account, AccountNumber, AccountType};

pub const CASH: Account = Account::new(AccountNumber::new(1000), "Cash", AccountType::Asset);

pub const PAYMENT_PROVIDER_CLEARING: Account = Account::new(
    AccountNumber::new(1010),
    "Payment Provider Clearing",
    AccountType::Asset,
);

pub const DEFERRED_REVENUE_SUBSCRIPTIONS: Account = Account::new(
    AccountNumber::new(2000),
    "Deferred Revenue: Subscriptions",
    AccountType::Liability,
);

pub const RETAINED_EARNINGS: Account = Account::new(
    AccountNumber::new(3000),
    "Retained Earnings",
    AccountType::Equity,
);

pub const SUBSCRIPTION_REVENUE: Account = Account::new(
    AccountNumber::new(4000),
    "Subscription Revenue",
    AccountType::Revenue,
);

pub const DISCOUNTS_AND_REFUNDS: Account = Account::contra(
    AccountNumber::new(4900),
    "Discounts and Refunds",
    AccountType::Revenue,
);

pub const PAYMENT_PROCESSING_FEES: Account = Account::new(
    AccountNumber::new(5000),
    "Payment Processing Fees",
    AccountType::Expense,
);
