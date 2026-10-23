//! Balances: computed from entries, never stored. This computation is the
//! definition that M3's kept-ready balances are rebuilt with and checked
//! against.

use std::fmt;

use chrono::{DateTime, Utc};

use crate::account::AccountNumber;
use crate::chart::Chart;
use crate::money::{Currency, Money};
use crate::tag::Tag;
use crate::transaction::PostedTransaction;

/// Which transactions count, by the two times on every transaction. Each
/// cutoff that is set means strictly before that time (ADR 0007); a
/// transaction counts only if it passes every cutoff that is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cutoffs {
    effective_before: Option<DateTime<Utc>>,
    recorded_before: Option<DateTime<Utc>>,
}

impl Cutoffs {
    /// No cutoffs: every transaction counts.
    pub const NONE: Self = Self {
        effective_before: None,
        recorded_before: None,
    };

    /// Counts only transactions effective strictly before `time`.
    pub const fn effective_before(self, time: DateTime<Utc>) -> Self {
        Self {
            effective_before: Some(time),
            ..self
        }
    }

    /// Counts only transactions recorded strictly before `time`.
    pub const fn recorded_before(self, time: DateTime<Utc>) -> Self {
        Self {
            recorded_before: Some(time),
            ..self
        }
    }

    /// Whether `transaction` passes every cutoff that is set.
    pub fn includes<C: Currency, T: Tag>(&self, transaction: &PostedTransaction<C, T>) -> bool {
        self.effective_before
            .is_none_or(|cutoff| transaction.effective_at() < cutoff)
            && self
                .recorded_before
                .is_none_or(|cutoff| transaction.recorded_at() < cutoff)
    }
}

/// A question about one account's balance: which account, optionally one
/// exact tag, and optionally cutoffs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalanceQuery<T: Tag> {
    account: AccountNumber,
    tag: Option<T>,
    cutoffs: Cutoffs,
}

impl<T: Tag> BalanceQuery<T> {
    /// The balance of `account` over all its entries.
    pub const fn account(account: AccountNumber) -> Self {
        Self {
            account,
            tag: None,
            cutoffs: Cutoffs::NONE,
        }
    }

    /// Counts only entries carrying exactly this tag (kind and value). Without
    /// a tag, every entry on the account counts, tagged or not.
    pub fn tagged(self, tag: T) -> Self {
        Self {
            tag: Some(tag),
            ..self
        }
    }

    /// Counts only transactions effective strictly before `time`.
    pub fn effective_before(self, time: DateTime<Utc>) -> Self {
        let cutoffs = self.cutoffs.effective_before(time);
        self.cutoffs(cutoffs)
    }

    /// Counts only transactions recorded strictly before `time`.
    pub fn recorded_before(self, time: DateTime<Utc>) -> Self {
        let cutoffs = self.cutoffs.recorded_before(time);
        self.cutoffs(cutoffs)
    }

    /// Replaces the cutoffs.
    pub fn cutoffs(self, cutoffs: Cutoffs) -> Self {
        Self { cutoffs, ..self }
    }
}

/// The balance of one account: a signed amount on the account's normal side.
/// Entries on the normal side add and entries on the other side subtract, so
/// a negative balance sits on the abnormal side, which is valid. The normal
/// side comes from `chart`, and an account missing from it is an error.
pub fn balance<'a, C: Currency + 'a, T: Tag + 'a>(
    transactions: impl IntoIterator<Item = &'a PostedTransaction<C, T>>,
    chart: &Chart,
    query: &BalanceQuery<T>,
) -> Result<Money<C>, BalanceError> {
    let normal_side = chart
        .get(query.account)
        .ok_or(BalanceError::UnknownAccount {
            account: query.account,
        })?
        .normal_side();
    transactions
        .into_iter()
        .filter(|transaction| query.cutoffs.includes(transaction))
        .flat_map(|transaction| transaction.entries())
        .filter(|entry| entry.account() == query.account)
        .filter(|entry| {
            query
                .tag
                .as_ref()
                .is_none_or(|tag| entry.tags().contains(tag))
        })
        .try_fold(Money::from_minor_units(0), |total, entry| {
            if entry.side() == normal_side {
                total.checked_add(entry.amount())
            } else {
                total.checked_sub(entry.amount())
            }
            .map_err(|_| BalanceError::Overflow)
        })
}

/// Why a balance couldn't be computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalanceError {
    /// The queried account isn't in the chart.
    UnknownAccount { account: AccountNumber },
    /// The balance doesn't fit in the ledger's integer amounts.
    Overflow,
}

impl fmt::Display for BalanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BalanceError::UnknownAccount { account } => {
                write!(f, "account {} is not in the chart", account.get())
            }
            BalanceError::Overflow => {
                write!(
                    f,
                    "balance is too large or too small for the ledger to hold"
                )
            }
        }
    }
}

impl std::error::Error for BalanceError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Usd;
    use crate::test_support::*;
    use crate::transaction::{TransactionId, UnpostedTransaction};
    use chrono::TimeDelta;
    use proptest::prelude::*;

    type Posted = PostedTransaction<Usd, TestTag>;

    /// Debit `debit` and credit `credit` by `cents`, both with `tags`.
    fn pair(
        debit: AccountNumber,
        credit: AccountNumber,
        cents: i64,
        tags: &[TestTag],
        effective_at: DateTime<Utc>,
        recorded_at: DateTime<Utc>,
        id: u64,
    ) -> Posted {
        UnpostedTransaction::builder(effective_at, "Fixture")
            .debit(debit, usd(cents), tags.iter().cloned())
            .credit(credit, usd(cents), tags.iter().cloned())
            .build(&chart(), effective_at)
            .unwrap()
            .post(&chart(), TransactionId::new(id), recorded_at)
            .unwrap()
    }

    /// Posts each `(debit, credit, cents)` at `noon()`, with IDs from 1.
    fn posted_at_noon(specs: &[(AccountNumber, AccountNumber, i64)]) -> Vec<Posted> {
        specs
            .iter()
            .zip(1..)
            .map(|(&(debit, credit, cents), id)| {
                pair(debit, credit, cents, &[], noon(), noon(), id)
            })
            .collect()
    }

    fn of(
        transactions: &[Posted],
        query: &BalanceQuery<TestTag>,
    ) -> Result<Money<Usd>, BalanceError> {
        balance(transactions, &chart(), query)
    }

    #[test]
    fn a_credit_normal_balance_reads_positive() {
        let transactions = posted_at_noon(&[
            (REVENUE, LIABILITY, 24_000),
            (LIABILITY, ASSET, 2_000),
            (LIABILITY, ASSET, 2_000),
        ]);
        assert_eq!(
            of(&transactions, &BalanceQuery::account(LIABILITY)),
            Ok(usd(20_000))
        );
    }

    #[test]
    fn a_debit_normal_balance_reads_positive() {
        let transactions = posted_at_noon(&[(ASSET, REVENUE, 1_000)]);
        assert_eq!(
            of(&transactions, &BalanceQuery::account(ASSET)),
            Ok(usd(1_000))
        );
    }

    #[test]
    fn a_contra_account_uses_its_own_normal_side() {
        let transactions = posted_at_noon(&[(CONTRA_REVENUE, ASSET, 500)]);
        assert_eq!(
            of(&transactions, &BalanceQuery::account(CONTRA_REVENUE)),
            Ok(usd(500))
        );
    }

    #[test]
    fn an_abnormal_balance_is_negative() {
        let transactions = posted_at_noon(&[
            (ASSET, REVENUE, 2_000),
            (LIABILITY, ASSET, 2_000),
            (LIABILITY, ASSET, 500),
        ]);
        assert_eq!(
            of(&transactions, &BalanceQuery::account(ASSET)),
            Ok(usd(-500))
        );
    }

    #[test]
    fn no_transactions_means_zero() {
        assert_eq!(of(&[], &BalanceQuery::account(ASSET)), Ok(usd(0)));
    }

    #[test]
    fn a_tag_balance_counts_only_entries_with_that_exact_tag() {
        let transactions = [
            pair(
                ASSET,
                LIABILITY,
                2_000,
                &[TestTag::Alpha(42), TestTag::Beta(9)],
                noon(),
                noon(),
                1,
            ),
            pair(
                ASSET,
                LIABILITY,
                3_000,
                &[TestTag::Alpha(43)],
                noon(),
                noon(),
                2,
            ),
            pair(LIABILITY, ASSET, 500, &[], noon(), noon(), 3),
        ];
        let liability = BalanceQuery::<TestTag>::account(LIABILITY);
        let tagged = |tag| of(&transactions, &liability.clone().tagged(tag));
        assert_eq!(tagged(TestTag::Alpha(42)), Ok(usd(2_000)));
        assert_eq!(tagged(TestTag::Alpha(43)), Ok(usd(3_000)));
        assert_eq!(tagged(TestTag::Beta(9)), Ok(usd(2_000)));
        assert_eq!(tagged(TestTag::Beta(42)), Ok(usd(0)));
        assert_eq!(of(&transactions, &liability), Ok(usd(4_500)));
    }

    #[test]
    fn cutoffs_are_strictly_before() {
        let transactions = posted_at_noon(&[(ASSET, REVENUE, 700)]);
        let query = BalanceQuery::<TestTag>::account(ASSET);
        let later = noon() + TimeDelta::nanoseconds(1);
        assert_eq!(
            of(&transactions, &query.clone().effective_before(noon())),
            Ok(usd(0))
        );
        assert_eq!(
            of(&transactions, &query.clone().effective_before(later)),
            Ok(usd(700))
        );
        assert_eq!(
            of(&transactions, &query.clone().recorded_before(noon())),
            Ok(usd(0))
        );
        assert_eq!(
            of(&transactions, &query.recorded_before(later)),
            Ok(usd(700))
        );
    }

    #[test]
    fn the_cutoffs_combine() {
        let earlier = noon() - TimeDelta::days(15);
        let transactions = [
            pair(ASSET, REVENUE, 100, &[], earlier, earlier, 1),
            pair(
                ASSET,
                REVENUE,
                20,
                &[],
                noon(),
                noon() + TimeDelta::days(2),
                2,
            ),
        ];
        let query = BalanceQuery::<TestTag>::account(ASSET);
        let effective = noon() + TimeDelta::seconds(1);
        let recorded = noon() + TimeDelta::days(1);
        assert_eq!(
            of(&transactions, &query.clone().effective_before(effective)),
            Ok(usd(120))
        );
        assert_eq!(
            of(&transactions, &query.clone().recorded_before(recorded)),
            Ok(usd(100))
        );
        assert_eq!(
            of(
                &transactions,
                &query.effective_before(effective).recorded_before(recorded)
            ),
            Ok(usd(100))
        );
    }

    #[test]
    fn rejects_an_account_missing_from_the_chart() {
        assert_eq!(
            of(&[], &BalanceQuery::account(NOT_IN_CHART)),
            Err(BalanceError::UnknownAccount {
                account: NOT_IN_CHART
            })
        );
    }

    #[test]
    fn rejects_a_balance_that_overflows() {
        let transactions =
            posted_at_noon(&[(ASSET, REVENUE, i64::MAX), (ASSET, REVENUE, i64::MAX)]);
        assert_eq!(
            of(&transactions, &BalanceQuery::account(ASSET)),
            Err(BalanceError::Overflow)
        );
    }

    #[test]
    fn balance_errors_explain_themselves() {
        assert_eq!(
            BalanceError::UnknownAccount {
                account: NOT_IN_CHART
            }
            .to_string(),
            "account 99 is not in the chart"
        );
        assert_eq!(
            BalanceError::Overflow.to_string(),
            "balance is too large or too small for the ledger to hold"
        );
    }

    proptest! {
        #[test]
        fn every_balance_matches_a_plain_sum(
            transactions in posted_transactions(),
            account in 1_u32..=4,
            tag in prop::option::of(any_test_tag()),
            cutoffs in any_cutoffs(),
        ) {
            let account = AccountNumber::new(account);
            let normal_side = chart().get(account).unwrap().normal_side();
            // Re-implemented from the two times, not via `Cutoffs::includes`.
            let (effective, recorded) = (cutoffs.effective_before, cutoffs.recorded_before);
            let mut oracle: i128 = 0;
            for transaction in &transactions {
                if effective.is_some_and(|cutoff| transaction.effective_at() >= cutoff)
                    || recorded.is_some_and(|cutoff| transaction.recorded_at() >= cutoff)
                {
                    continue;
                }
                for entry in transaction.entries() {
                    let tag_matches = tag.as_ref().is_none_or(|t| entry.tags().contains(t));
                    if entry.account() == account && tag_matches {
                        let cents = i128::from(entry.amount().minor_units());
                        if entry.side() == normal_side {
                            oracle += cents;
                        } else {
                            oracle -= cents;
                        }
                    }
                }
            }
            let mut query = BalanceQuery::account(account).cutoffs(cutoffs);
            if let Some(tag) = tag {
                query = query.tagged(tag);
            }
            prop_assert_eq!(
                balance(&transactions, &chart(), &query),
                Ok(usd(i64::try_from(oracle).unwrap()))
            );
        }
    }
}
