//! Fixtures shared by the crate's unit tests: a small chart, tags, amounts, a
//! fixed time, and generators for valid transactions. Test-only.

use chrono::{DateTime, TimeZone, Utc};
use proptest::prelude::*;

use crate::account::{Account, AccountNumber, AccountType, Side};
use crate::chart::Chart;
use crate::money::{Money, Usd};
use crate::tag::Tag;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TestTag {
    Alpha(u64),
    Beta(u64),
}

impl Tag for TestTag {
    fn kind(&self) -> &'static str {
        match self {
            TestTag::Alpha(_) => "alpha",
            TestTag::Beta(_) => "beta",
        }
    }
}

pub(crate) const ASSET: AccountNumber = AccountNumber::new(1);
pub(crate) const LIABILITY: AccountNumber = AccountNumber::new(2);
pub(crate) const REVENUE: AccountNumber = AccountNumber::new(3);
/// A contra-revenue account: a revenue account whose normal side is debit.
pub(crate) const CONTRA_REVENUE: AccountNumber = AccountNumber::new(4);
pub(crate) const NOT_IN_CHART: AccountNumber = AccountNumber::new(99);

pub(crate) fn chart() -> Chart {
    Chart::new([
        Account::new(ASSET, "Asset", AccountType::Asset),
        Account::new(LIABILITY, "Liability", AccountType::Liability),
        Account::new(REVENUE, "Revenue", AccountType::Revenue),
        Account::contra(CONTRA_REVENUE, "Contra revenue", AccountType::Revenue),
    ])
    .unwrap()
}

pub(crate) fn usd(cents: i64) -> Money<Usd> {
    Money::from_minor_units(cents)
}

/// 2026-10-16 12:00:00 UTC: the default effective time and `now`, which
/// tests move earlier or later when they need to.
pub(crate) fn noon() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap()
}

/// An entry before it's added to a builder: account, side, amount in
/// cents, tags.
pub(crate) type TestEntry = (AccountNumber, Side, i64, Vec<TestTag>);

/// One balanced group of entries: a debit of `x` and a credit of `x`, or a
/// debit of `x + y` split into credits of `x` and `y`. The last two values
/// are the debit and credit account numbers, both in the chart.
pub(crate) fn balanced_group() -> impl Strategy<Value = (i64, Option<i64>, u32, u32)> {
    (
        1_i64..=1_000_000,
        prop::option::of(1_i64..=1_000_000),
        1_u32..=4,
        1_u32..=4,
    )
}

/// A valid transaction's entries: one to three balanced groups, every
/// entry carrying the same tags, at most one of each kind.
pub(crate) fn valid_entries() -> impl Strategy<Value = Vec<TestEntry>> {
    (
        prop::collection::vec(balanced_group(), 1..4),
        prop::option::of(0_u64..100),
        prop::option::of(0_u64..100),
    )
        .prop_map(|(groups, alpha, beta)| {
            let tags: Vec<TestTag> = alpha
                .map(TestTag::Alpha)
                .into_iter()
                .chain(beta.map(TestTag::Beta))
                .collect();
            let mut entries = Vec::new();
            for (x, y, debit, credit) in groups {
                let (debit, credit) = (AccountNumber::new(debit), AccountNumber::new(credit));
                match y {
                    None => {
                        entries.push((debit, Side::Debit, x, tags.clone()));
                        entries.push((credit, Side::Credit, x, tags.clone()));
                    }
                    Some(y) => {
                        entries.push((debit, Side::Debit, x + y, tags.clone()));
                        entries.push((credit, Side::Credit, x, tags.clone()));
                        entries.push((credit, Side::Credit, y, tags.clone()));
                    }
                }
            }
            entries
        })
}
