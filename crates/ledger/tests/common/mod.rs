//! The playground: accounts from the reference business's chart of accounts
//! and its tag kinds, shared by every test that needs them. They live in
//! tests, not in the ledger, because the ledger knows nothing about the
//! business it serves (D6).

#![allow(
    dead_code,
    reason = "each test file compiles this module separately and uses only part of it"
)]

use ledger::{Account, AccountNumber, AccountType, Chart, Tag};

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

/// Every playground account, for building the playground chart.
pub const PLAYGROUND_ACCOUNTS: [Account; 7] = [
    CASH,
    PAYMENT_PROVIDER_CLEARING,
    DEFERRED_REVENUE_SUBSCRIPTIONS,
    RETAINED_EARNINGS,
    SUBSCRIPTION_REVENUE,
    DISCOUNTS_AND_REFUNDS,
    PAYMENT_PROCESSING_FEES,
];

/// The playground chart: every playground account, keyed by number.
pub fn playground_chart() -> Chart {
    Chart::new(PLAYGROUND_ACCOUNTS).expect("playground account numbers are unique")
}

/// A customer's ID. Its own type, so it can't be passed where a subscription
/// ID belongs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CustomerId(pub u64);

/// A subscription's ID. Its own type, so it can't be passed where a customer
/// ID belongs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub u64);

/// The playground's tag kinds (Ledger Core Spec: "the playground uses
/// `customer` and `subscription`"). Each carries its own ID type, so
/// `Customer(SubscriptionId(9))` doesn't compile (ADR 0005). A real business
/// crate supplies its own.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PlaygroundTag {
    Customer(CustomerId),
    Subscription(SubscriptionId),
}

impl Tag for PlaygroundTag {
    fn kind(&self) -> &'static str {
        match self {
            PlaygroundTag::Customer(_) => "customer",
            PlaygroundTag::Subscription(_) => "subscription",
        }
    }
}
