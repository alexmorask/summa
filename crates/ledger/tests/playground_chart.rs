//! Checks the playground chart against the Ledger Core Spec's table.

mod common;

use std::collections::HashSet;

use common::{
    CASH, DEFERRED_REVENUE_SUBSCRIPTIONS, DISCOUNTS_AND_REFUNDS, PAYMENT_PROCESSING_FEES,
    PAYMENT_PROVIDER_CLEARING, RETAINED_EARNINGS, SUBSCRIPTION_REVENUE,
};
use ledger::{AccountType, Side};

#[test]
fn playground_chart_matches_the_spec() {
    let expected = [
        (CASH, 1000, "Cash", AccountType::Asset, false, Side::Debit),
        (
            PAYMENT_PROVIDER_CLEARING,
            1010,
            "Payment Provider Clearing",
            AccountType::Asset,
            false,
            Side::Debit,
        ),
        (
            DEFERRED_REVENUE_SUBSCRIPTIONS,
            2000,
            "Deferred Revenue: Subscriptions",
            AccountType::Liability,
            false,
            Side::Credit,
        ),
        (
            RETAINED_EARNINGS,
            3000,
            "Retained Earnings",
            AccountType::Equity,
            false,
            Side::Credit,
        ),
        (
            SUBSCRIPTION_REVENUE,
            4000,
            "Subscription Revenue",
            AccountType::Revenue,
            false,
            Side::Credit,
        ),
        (
            DISCOUNTS_AND_REFUNDS,
            4900,
            "Discounts and Refunds",
            AccountType::Revenue,
            true,
            Side::Debit,
        ),
        (
            PAYMENT_PROCESSING_FEES,
            5000,
            "Payment Processing Fees",
            AccountType::Expense,
            false,
            Side::Debit,
        ),
    ];

    for (account, number, name, account_type, contra, normal_side) in expected {
        assert_eq!(account.name(), name);
        assert_eq!(account.number().get(), number, "{name}");
        assert_eq!(account.account_type(), account_type, "{name}");
        assert_eq!(account.is_contra(), contra, "{name}");
        assert_eq!(account.normal_side(), normal_side, "{name}");
    }
}

#[test]
fn playground_account_numbers_are_unique() {
    let chart = [
        CASH,
        PAYMENT_PROVIDER_CLEARING,
        DEFERRED_REVENUE_SUBSCRIPTIONS,
        RETAINED_EARNINGS,
        SUBSCRIPTION_REVENUE,
        DISCOUNTS_AND_REFUNDS,
        PAYMENT_PROCESSING_FEES,
    ];
    let numbers: HashSet<_> = chart.iter().map(|a| a.number()).collect();
    assert_eq!(numbers.len(), chart.len());
}
