//! Balances and the trial balance from the Ledger Core Spec, built with the
//! playground's chart and tags.

mod common;

use chrono::{DateTime, TimeZone, Utc};
use common::{
    CASH, DEFERRED_REVENUE_SUBSCRIPTIONS, DISCOUNTS_AND_REFUNDS, PAYMENT_PROCESSING_FEES,
    PAYMENT_PROVIDER_CLEARING, PlaygroundTag, RETAINED_EARNINGS, SUBSCRIPTION_REVENUE,
    playground_chart,
};
use ledger::{
    AccountNumber, BalanceQuery, Cutoffs, Money, PostedTransaction, Side, TransactionId,
    UnpostedTransaction, Usd, balance, trial_balance,
};

fn usd(cents: i64) -> Money<Usd> {
    Money::from_minor_units(cents)
}

fn at(month: u32, day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, month, day, hour, 0, 0).unwrap()
}

/// Posts a two-entry transaction: `amount` debited to `debit` and credited to
/// `credit`.
fn transfer(
    id: u64,
    debit: AccountNumber,
    credit: AccountNumber,
    amount: i64,
    effective_at: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
) -> PostedTransaction<Usd, PlaygroundTag> {
    let chart = playground_chart();
    UnpostedTransaction::builder(effective_at, "transfer")
        .debit(debit, usd(amount), [])
        .credit(credit, usd(amount), [])
        .build(&chart, recorded_at)
        .unwrap()
        .post_for_tests(&chart, TransactionId::new(id), recorded_at)
        .unwrap()
}

#[test]
fn deferred_revenue_is_240_less_two_months() {
    let chart = playground_chart();
    let ppc = PAYMENT_PROVIDER_CLEARING.number();
    let drs = DEFERRED_REVENUE_SUBSCRIPTIONS.number();
    let revenue = SUBSCRIPTION_REVENUE.number();
    let now = at(12, 1, 0);
    let transactions = [
        transfer(1, ppc, drs, 24_000, at(10, 1, 0), now),
        transfer(2, drs, revenue, 2_000, at(10, 31, 0), now),
        transfer(3, drs, revenue, 2_000, at(11, 30, 0), now),
    ];

    let query = |account| BalanceQuery::<PlaygroundTag>::account(account);
    assert_eq!(
        balance(&transactions, &chart, &query(drs)).unwrap(),
        usd(20_000)
    );
    assert_eq!(
        balance(&transactions, &chart, &query(revenue)).unwrap(),
        usd(4_000)
    );
}

#[test]
fn the_late_fee_by_each_time() {
    let chart = playground_chart();
    let fees = PAYMENT_PROCESSING_FEES.number();
    let ppc = PAYMENT_PROVIDER_CLEARING.number();
    let transactions = [
        transfer(1, fees, ppc, 10_000, at(9, 15, 0), at(9, 15, 0)),
        transfer(2, fees, ppc, 93, at(9, 30, 23), at(10, 2, 9)),
    ];

    let query = BalanceQuery::<PlaygroundTag>::account(fees);
    let by = |query| balance(&transactions, &chart, &query).unwrap();
    assert_eq!(
        by(query.clone().effective_before(at(10, 1, 0))),
        usd(10_093)
    );
    assert_eq!(by(query.clone().recorded_before(at(10, 2, 0))), usd(10_000));
    assert_eq!(
        by(query
            .clone()
            .effective_before(at(10, 1, 0))
            .recorded_before(at(10, 2, 0))),
        usd(10_000)
    );
    assert_eq!(by(query), usd(10_093));
}

#[test]
fn the_playground_trial_balance() {
    let chart = playground_chart();
    let t = at(10, 1, 0);
    let transactions = [
        transfer(
            1,
            PAYMENT_PROVIDER_CLEARING.number(),
            DEFERRED_REVENUE_SUBSCRIPTIONS.number(),
            2_000,
            t,
            t,
        ),
        transfer(
            2,
            DEFERRED_REVENUE_SUBSCRIPTIONS.number(),
            SUBSCRIPTION_REVENUE.number(),
            2_000,
            t,
            t,
        ),
        transfer(
            3,
            DISCOUNTS_AND_REFUNDS.number(),
            PAYMENT_PROVIDER_CLEARING.number(),
            500,
            t,
            t,
        ),
        transfer(
            4,
            CASH.number(),
            PAYMENT_PROVIDER_CLEARING.number(),
            2_000,
            t,
            t,
        ),
        transfer(5, PAYMENT_PROCESSING_FEES.number(), CASH.number(), 88, t, t),
    ];

    let trial = trial_balance(&transactions, &chart, Cutoffs::NONE).unwrap();

    let lines: Vec<_> = trial
        .lines()
        .iter()
        .map(|line| (line.account(), line.side(), line.amount()))
        .collect();
    assert_eq!(
        lines,
        [
            (CASH.number(), Side::Debit, usd(1_912)),
            (PAYMENT_PROVIDER_CLEARING.number(), Side::Credit, usd(500)),
            (
                DEFERRED_REVENUE_SUBSCRIPTIONS.number(),
                Side::Credit,
                usd(0)
            ),
            (RETAINED_EARNINGS.number(), Side::Credit, usd(0)),
            (SUBSCRIPTION_REVENUE.number(), Side::Credit, usd(2_000)),
            (DISCOUNTS_AND_REFUNDS.number(), Side::Debit, usd(500)),
            (PAYMENT_PROCESSING_FEES.number(), Side::Debit, usd(88)),
        ]
    );
    assert_eq!(trial.total_debits(), usd(2_500));
    assert_eq!(trial.total_credits(), usd(2_500));
    assert!(trial.is_balanced());
}
