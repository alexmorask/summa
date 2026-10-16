//! The worked charge from the Ledger Core Spec, built with the playground's
//! chart and tags: $21.60 = $20.00 + $1.60.

mod common;

use chrono::{DateTime, TimeZone, Utc};
use common::PlaygroundTag::{Customer, Subscription};
use common::{
    CustomerId, DEFERRED_REVENUE_SUBSCRIPTIONS, PAYMENT_PROVIDER_CLEARING, PLAYGROUND_ACCOUNTS,
    PlaygroundTag, SubscriptionId, playground_chart,
};
use ledger::{
    Account, AccountNumber, AccountType, Chart, Money, Side, TransactionBuilder, TransactionError,
    UnpostedTransaction, Usd,
};

/// Sales Tax Payable isn't a playground account (the Ledger Core Spec uses it
/// only in this example), so it's defined here with its number from the Chart
/// of Accounts.
const SALES_TAX_PAYABLE: Account = Account::new(
    AccountNumber::new(2100),
    "Sales Tax Payable",
    AccountType::Liability,
);

fn chart_with_sales_tax() -> Chart {
    Chart::new(PLAYGROUND_ACCOUNTS.into_iter().chain([SALES_TAX_PAYABLE])).unwrap()
}

fn usd(cents: i64) -> Money<Usd> {
    Money::from_minor_units(cents)
}

fn september_30() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 30, 15, 0, 0).unwrap()
}

const CUSTOMER_42: PlaygroundTag = Customer(CustomerId(42));
const SUBSCRIPTION_9: PlaygroundTag = Subscription(SubscriptionId(9));

/// The charge's first two entries: $21.60 debited to Payment Provider
/// Clearing, $20.00 credited to Deferred Revenue: Subscriptions.
fn charge_without_tax() -> TransactionBuilder<Usd, PlaygroundTag> {
    UnpostedTransaction::builder(september_30(), "Customer 42 buys Pro monthly")
        .debit(
            PAYMENT_PROVIDER_CLEARING.number(),
            usd(2_160),
            [CUSTOMER_42],
        )
        .credit(
            DEFERRED_REVENUE_SUBSCRIPTIONS.number(),
            usd(2_000),
            [CUSTOMER_42, SUBSCRIPTION_9],
        )
}

/// The full charge: the two entries above plus $1.60 credited to Sales Tax
/// Payable.
fn charge() -> TransactionBuilder<Usd, PlaygroundTag> {
    charge_without_tax().credit(SALES_TAX_PAYABLE.number(), usd(160), [CUSTOMER_42])
}

#[test]
fn the_three_entry_charge_balances() {
    // Debits: 2,160 cents. Credits: 2,000 + 160 = 2,160 cents.
    let t = charge()
        .build(&chart_with_sales_tax(), september_30())
        .unwrap();

    let entries = t.entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].side(), Side::Debit);
    assert_eq!(entries[0].amount(), usd(2_160));
    assert_eq!(entries[1].side(), Side::Credit);
    assert_eq!(entries[1].amount(), usd(2_000));
    assert_eq!(entries[1].tags(), &[CUSTOMER_42, SUBSCRIPTION_9]);
    assert_eq!(entries[2].account(), SALES_TAX_PAYABLE.number());
    assert_eq!(entries[2].amount(), usd(160));
    assert_eq!(t.effective_at(), september_30());
}

#[test]
fn the_charge_without_its_tax_is_rejected() {
    let error = charge_without_tax()
        .build(&chart_with_sales_tax(), september_30())
        .unwrap_err();
    assert_eq!(
        error,
        TransactionError::Unbalanced {
            debits: usd(2_160),
            credits: usd(2_000),
        }
    );
    assert_eq!(error.to_string(), "debits 21.60 USD ≠ credits 20.00 USD");
}

#[test]
fn sales_tax_payable_is_not_in_the_playground_chart() {
    let error = charge()
        .build(&playground_chart(), september_30())
        .unwrap_err();
    assert_eq!(
        error,
        TransactionError::UnknownAccount {
            entry: 2,
            account: SALES_TAX_PAYABLE.number(),
        }
    );
}

#[test]
fn playground_tags_name_their_kinds() {
    use ledger::Tag;
    assert_eq!(CUSTOMER_42.kind(), "customer");
    assert_eq!(SUBSCRIPTION_9.kind(), "subscription");
}
