//! Fixtures shared by the crate's unit tests: a small chart, tags, amounts, a
//! fixed time, and generators for valid transactions. Test-only.

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use proptest::prelude::*;

use crate::account::{Account, AccountNumber, AccountType, Side};
use crate::balance::Cutoffs;
use crate::chart::Chart;
use crate::idempotency::IdempotencyKey;
use crate::money::{Money, Usd};
use crate::tag::Tag;
use crate::transaction::{PostedTransaction, TransactionId, UnpostedTransaction};

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

/// Tag values are the decimal `u64`.
#[cfg(feature = "postgres")]
impl crate::store::StoredTag for TestTag {
    fn value(&self) -> String {
        match self {
            TestTag::Alpha(n) | TestTag::Beta(n) => n.to_string(),
        }
    }

    fn from_stored(kind: &str, value: &str) -> Option<Self> {
        let n = value.parse().ok()?;
        match kind {
            "alpha" => Some(TestTag::Alpha(n)),
            "beta" => Some(TestTag::Beta(n)),
            _ => None,
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

/// A valid idempotency key from text, for tests.
pub(crate) fn key(text: &str) -> IdempotencyKey {
    IdempotencyKey::new(text).unwrap()
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

/// A tag of either kind, with a value in `0..3` so tags repeat often.
pub(crate) fn any_test_tag() -> impl Strategy<Value = TestTag> {
    prop_oneof![
        (0_u64..3).prop_map(TestTag::Alpha),
        (0_u64..3).prop_map(TestTag::Beta),
    ]
}

/// Like [`valid_entries`], but each entry gets its own optional `Alpha` and
/// optional `Beta` tag (values in `0..3`), so tags differ between entries and
/// repeat across transactions.
pub(crate) fn valid_entries_with_varied_tags() -> impl Strategy<Value = Vec<TestEntry>> {
    // valid_entries makes at most three groups of three entries.
    (
        valid_entries(),
        prop::collection::vec((prop::option::of(0_u64..3), prop::option::of(0_u64..3)), 9),
    )
        .prop_map(|(entries, tags)| {
            entries
                .into_iter()
                .zip(tags)
                .map(|((account, side, cents, _), (alpha, beta))| {
                    let tags = alpha
                        .map(TestTag::Alpha)
                        .into_iter()
                        .chain(beta.map(TestTag::Beta))
                        .collect();
                    (account, side, cents, tags)
                })
                .collect()
        })
}

/// Zero to eight posted transactions with IDs `1..=n`. Each is effective up
/// to 30 days before `noon()` and recorded up to 5 days after that.
pub(crate) fn posted_transactions() -> impl Strategy<Value = Vec<PostedTransaction<Usd, TestTag>>> {
    prop::collection::vec(
        (
            valid_entries_with_varied_tags(),
            0_i64..=30 * 86_400,
            0_i64..=5 * 86_400,
        ),
        0..=8,
    )
    .prop_map(|specs| {
        specs
            .into_iter()
            .enumerate()
            .map(|(index, (entries, earlier, later))| {
                let effective_at = noon() - TimeDelta::seconds(earlier);
                let recorded_at = effective_at + TimeDelta::seconds(later);
                entries
                    .into_iter()
                    .fold(
                        UnpostedTransaction::builder(
                            key(&format!("generated-{index}")),
                            effective_at,
                            "Generated",
                        ),
                        |builder, (account, side, cents, tags)| match side {
                            Side::Debit => builder.debit(account, usd(cents), tags),
                            Side::Credit => builder.credit(account, usd(cents), tags),
                        },
                    )
                    .build(&chart(), effective_at)
                    .unwrap()
                    .post(&chart(), TransactionId::new(index as u64 + 1), recorded_at)
                    .unwrap()
            })
            .collect()
    })
}

/// Cutoffs where each is unset or up to 35 days before `noon()`.
pub(crate) fn any_cutoffs() -> impl Strategy<Value = Cutoffs> {
    (
        prop::option::of(0_i64..=35 * 86_400),
        prop::option::of(0_i64..=35 * 86_400),
    )
        .prop_map(|(effective, recorded)| {
            let mut cutoffs = Cutoffs::NONE;
            if let Some(seconds) = effective {
                cutoffs = cutoffs.effective_before(noon() - TimeDelta::seconds(seconds));
            }
            if let Some(seconds) = recorded {
                cutoffs = cutoffs.recorded_before(noon() - TimeDelta::seconds(seconds));
            }
            cutoffs
        })
}
