//! Reversals from the Ledger Core Spec, built with the playground's chart and
//! tags.

mod common;

use chrono::{DateTime, TimeZone, Utc};
use common::PlaygroundTag::{Customer, Subscription};
use common::{
    CustomerId, DEFERRED_REVENUE_SUBSCRIPTIONS, PAYMENT_PROVIDER_CLEARING, PlaygroundTag,
    SALES_TAX_PAYABLE, SubscriptionId, chart_with_sales_tax,
};
use ledger::{
    BalanceQuery, Money, PostedTransaction, ReversalError, TransactionId, UnpostedTransaction, Usd,
    balance,
};

fn usd(cents: i64) -> Money<Usd> {
    Money::from_minor_units(cents)
}

fn at(month: u32, day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, month, day, hour, 0, 0).unwrap()
}

const CUSTOMER_42: PlaygroundTag = Customer(CustomerId(42));
const CUSTOMER_43: PlaygroundTag = Customer(CustomerId(43));
const SUBSCRIPTION_9: PlaygroundTag = Subscription(SubscriptionId(9));

/// The spec's charge, tagged to the given customer.
fn charge(customer: PlaygroundTag) -> UnpostedTransaction<Usd, PlaygroundTag> {
    UnpostedTransaction::builder(at(9, 30, 15), "Customer buys Pro monthly")
        .debit(
            PAYMENT_PROVIDER_CLEARING.number(),
            usd(2_160),
            [customer.clone()],
        )
        .credit(
            DEFERRED_REVENUE_SUBSCRIPTIONS.number(),
            usd(2_000),
            [customer.clone(), SUBSCRIPTION_9],
        )
        .credit(SALES_TAX_PAYABLE.number(), usd(160), [customer])
        .build(&chart_with_sales_tax(), at(9, 30, 15))
        .unwrap()
}

fn post(
    transaction: UnpostedTransaction<Usd, PlaygroundTag>,
    id: u64,
    recorded_at: DateTime<Utc>,
) -> PostedTransaction<Usd, PlaygroundTag> {
    transaction
        .post_for_tests(&chart_with_sales_tax(), TransactionId::new(id), recorded_at)
        .unwrap()
}

#[test]
fn the_wrong_customer_is_corrected_and_nets_to_zero() {
    let chart = chart_with_sales_tax();
    let t1 = post(charge(CUSTOMER_42), 1, at(9, 30, 15));
    let t2 = post(
        t1.reversal(
            at(10, 2, 9),
            "Reverses 1: wrong customer",
            &chart,
            at(10, 2, 9),
        )
        .unwrap(),
        2,
        at(10, 2, 9),
    );
    let t3 = post(charge(CUSTOMER_43), 3, at(10, 2, 9));
    let transactions = [t1, t2, t3];

    let total = |account, query: fn(BalanceQuery<PlaygroundTag>) -> BalanceQuery<PlaygroundTag>| {
        balance(
            &transactions,
            &chart,
            &query(BalanceQuery::account(account)),
        )
        .unwrap()
    };
    let accounts = [
        PAYMENT_PROVIDER_CLEARING.number(),
        DEFERRED_REVENUE_SUBSCRIPTIONS.number(),
        SALES_TAX_PAYABLE.number(),
    ];

    let customer_42: Vec<_> = accounts
        .iter()
        .map(|&a| total(a, |q| q.tagged(CUSTOMER_42)))
        .collect();
    assert_eq!(customer_42, [usd(0), usd(0), usd(0)]);

    let customer_43: Vec<_> = accounts
        .iter()
        .map(|&a| total(a, |q| q.tagged(CUSTOMER_43)))
        .collect();
    assert_eq!(customer_43, [usd(2_160), usd(2_000), usd(160)]);

    let everyone: Vec<_> = accounts.iter().map(|&a| total(a, |q| q)).collect();
    assert_eq!(everyone, [usd(2_160), usd(2_000), usd(160)]);

    let subscription_9 = total(DEFERRED_REVENUE_SUBSCRIPTIONS.number(), |q| {
        q.tagged(SUBSCRIPTION_9)
    });
    assert_eq!(subscription_9, usd(2_000));

    // Before the correction was recorded, the books showed the mistake.
    let before_correction = balance(
        &transactions,
        &chart,
        &BalanceQuery::account(DEFERRED_REVENUE_SUBSCRIPTIONS.number())
            .tagged(CUSTOMER_42)
            .recorded_before(at(10, 2, 0)),
    )
    .unwrap();
    assert_eq!(before_correction, usd(2_000));
}

#[test]
fn a_reversal_tagged_to_the_wrong_customer_is_rejected() {
    let chart = chart_with_sales_tax();
    let t1 = post(charge(CUSTOMER_42), 1, at(9, 30, 15));
    let t2 = UnpostedTransaction::builder(at(10, 2, 9), "Reverses 1: wrong customer")
        .reversing(TransactionId::new(1))
        .credit(
            PAYMENT_PROVIDER_CLEARING.number(),
            usd(2_160),
            [CUSTOMER_43],
        )
        .debit(
            DEFERRED_REVENUE_SUBSCRIPTIONS.number(),
            usd(2_000),
            [CUSTOMER_43, SUBSCRIPTION_9],
        )
        .debit(SALES_TAX_PAYABLE.number(), usd(160), [CUSTOMER_43])
        .build(&chart, at(10, 2, 9))
        .unwrap();

    assert_eq!(
        t2.check_reversal_of(&t1),
        Err(ReversalError::UnmatchedOriginalEntry { entry: 0 })
    );
}
