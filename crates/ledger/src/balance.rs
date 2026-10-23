//! Balances: computed from entries, never stored. This computation is the
//! definition that M3's kept-ready balances are rebuilt with and checked
//! against.

use std::fmt;

use chrono::{DateTime, Utc};

use crate::account::{AccountNumber, Side};
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

/// One account's line in a trial balance: the side the balance actually sits
/// on, and its size. The amount is never negative; an abnormal balance sits on
/// the opposite side of the account's normal side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrialBalanceLine<C: Currency> {
    account: AccountNumber,
    side: Side,
    amount: Money<C>,
}

impl<C: Currency> TrialBalanceLine<C> {
    /// The account this line is for.
    pub const fn account(&self) -> AccountNumber {
        self.account
    }

    /// The side the balance sits on.
    pub const fn side(&self) -> Side {
        self.side
    }

    /// The size of the balance, never negative.
    pub const fn amount(&self) -> Money<C> {
        self.amount
    }
}

/// Every account's balance, each on the side it sits on. For a balanced
/// ledger the debit and credit totals are equal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrialBalance<C: Currency> {
    lines: Vec<TrialBalanceLine<C>>,
    total_debits: Money<C>,
    total_credits: Money<C>,
}

impl<C: Currency> TrialBalance<C> {
    /// One line per account in the chart, in account number order.
    pub fn lines(&self) -> &[TrialBalanceLine<C>] {
        &self.lines
    }

    /// The sum of the lines on the debit side.
    pub const fn total_debits(&self) -> Money<C> {
        self.total_debits
    }

    /// The sum of the lines on the credit side.
    pub const fn total_credits(&self) -> Money<C> {
        self.total_credits
    }

    /// Whether the debit and credit totals are equal.
    pub fn is_balanced(&self) -> bool {
        self.total_debits == self.total_credits
    }
}

/// The trial balance of every account in `chart` over the transactions that
/// pass `cutoffs`. A balance on its normal side or at zero goes in that
/// side's column; a negative one goes in the opposite column, negated. An
/// entry whose account isn't in the chart is an error. An unbalanced result
/// is returned as it is, so the caller can see it.
pub fn trial_balance<'a, C: Currency + 'a, T: Tag + 'a>(
    transactions: impl IntoIterator<Item = &'a PostedTransaction<C, T>>,
    chart: &Chart,
    cutoffs: Cutoffs,
) -> Result<TrialBalance<C>, BalanceError> {
    let transactions: Vec<_> = transactions.into_iter().collect();
    for entry in transactions
        .iter()
        .filter(|transaction| cutoffs.includes(transaction))
        .flat_map(|transaction| transaction.entries())
    {
        if chart.get(entry.account()).is_none() {
            return Err(BalanceError::UnknownAccount {
                account: entry.account(),
            });
        }
    }
    let mut lines = Vec::new();
    let mut total_debits = Money::from_minor_units(0);
    let mut total_credits = Money::from_minor_units(0);
    for account in chart.accounts() {
        let query = BalanceQuery::<T>::account(account.number()).cutoffs(cutoffs);
        let signed = balance(transactions.iter().copied(), chart, &query)?;
        let (side, amount) = if signed.minor_units() >= 0 {
            (account.normal_side(), signed)
        } else {
            let negated = signed.checked_neg().map_err(|_| BalanceError::Overflow)?;
            (account.normal_side().opposite(), negated)
        };
        let total = match side {
            Side::Debit => &mut total_debits,
            Side::Credit => &mut total_credits,
        };
        *total = total
            .checked_add(amount)
            .map_err(|_| BalanceError::Overflow)?;
        lines.push(TrialBalanceLine {
            account: account.number(),
            side,
            amount,
        });
    }
    Ok(TrialBalance {
        lines,
        total_debits,
        total_credits,
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
    use crate::account::{Account, AccountType};
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

    /// The trial balance as `(account, side, cents)` triples.
    fn lines(trial_balance: &TrialBalance<Usd>) -> Vec<(AccountNumber, Side, i64)> {
        trial_balance
            .lines()
            .iter()
            .map(|line| (line.account(), line.side(), line.amount().minor_units()))
            .collect()
    }

    fn three_transactions(second_recorded_at: DateTime<Utc>) -> Vec<Posted> {
        vec![
            pair(ASSET, LIABILITY, 2_000, &[], noon(), noon(), 1),
            pair(
                LIABILITY,
                REVENUE,
                2_000,
                &[],
                noon(),
                second_recorded_at,
                2,
            ),
            pair(CONTRA_REVENUE, ASSET, 2_500, &[], noon(), noon(), 3),
        ]
    }

    #[test]
    fn places_each_balance_on_the_side_it_sits_on() {
        let transactions = three_transactions(noon());
        let trial_balance = trial_balance(&transactions, &chart(), Cutoffs::NONE).unwrap();
        assert_eq!(
            lines(&trial_balance),
            vec![
                (ASSET, Side::Credit, 500),
                (LIABILITY, Side::Credit, 0),
                (REVENUE, Side::Credit, 2_000),
                (CONTRA_REVENUE, Side::Debit, 2_500),
            ]
        );
        assert_eq!(trial_balance.total_debits(), usd(2_500));
        assert_eq!(trial_balance.total_credits(), usd(2_500));
        assert!(trial_balance.is_balanced());
    }

    #[test]
    fn no_transactions_lists_every_account_at_zero() {
        let transactions: Vec<Posted> = Vec::new();
        let trial_balance = trial_balance(&transactions, &chart(), Cutoffs::NONE).unwrap();
        assert_eq!(
            lines(&trial_balance),
            vec![
                (ASSET, Side::Debit, 0),
                (LIABILITY, Side::Credit, 0),
                (REVENUE, Side::Credit, 0),
                (CONTRA_REVENUE, Side::Debit, 0),
            ]
        );
        assert!(trial_balance.is_balanced());
    }

    #[test]
    fn respects_cutoffs() {
        let transactions = three_transactions(noon() + TimeDelta::days(1));
        let cutoffs = Cutoffs::NONE.recorded_before(noon() + TimeDelta::hours(1));
        let trial_balance = trial_balance(&transactions, &chart(), cutoffs).unwrap();
        assert_eq!(
            lines(&trial_balance),
            vec![
                (ASSET, Side::Credit, 500),
                (LIABILITY, Side::Credit, 2_000),
                (REVENUE, Side::Credit, 0),
                (CONTRA_REVENUE, Side::Debit, 2_500),
            ]
        );
        assert!(trial_balance.is_balanced());
    }

    #[test]
    fn rejects_an_entry_whose_account_is_missing_from_the_chart() {
        let transactions = three_transactions(noon());
        let without_revenue = Chart::new([
            Account::new(ASSET, "Asset", AccountType::Asset),
            Account::new(LIABILITY, "Liability", AccountType::Liability),
            Account::contra(CONTRA_REVENUE, "Contra revenue", AccountType::Revenue),
        ])
        .unwrap();
        assert_eq!(
            trial_balance(&transactions, &without_revenue, Cutoffs::NONE),
            Err(BalanceError::UnknownAccount { account: REVENUE })
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

        #[test]
        fn the_trial_balance_always_balances(
            transactions in posted_transactions(),
            cutoffs in any_cutoffs(),
        ) {
            let trial_balance = trial_balance(&transactions, &chart(), cutoffs).unwrap();
            prop_assert!(trial_balance.is_balanced());
            for line in trial_balance.lines() {
                prop_assert!(line.amount().minor_units() >= 0);
            }
        }

        #[test]
        fn a_transaction_and_its_reversal_net_to_zero(
            entries in valid_entries_with_varied_tags(),
        ) {
            let mut tags: Vec<TestTag> = Vec::new();
            for tag in entries.iter().flat_map(|(_, _, _, tags)| tags) {
                if !tags.contains(tag) {
                    tags.push(tag.clone());
                }
            }
            let original = entries
                .into_iter()
                .fold(
                    UnpostedTransaction::builder(noon(), "Original"),
                    |builder, (account, side, cents, tags)| match side {
                        Side::Debit => builder.debit(account, usd(cents), tags),
                        Side::Credit => builder.credit(account, usd(cents), tags),
                    },
                )
                .build(&chart(), noon())
                .unwrap()
                .post(&chart(), TransactionId::new(1), noon())
                .unwrap();
            let reversal = original
                .reversal(noon(), "Reverses 1", &chart(), noon())
                .unwrap()
                .post(&chart(), TransactionId::new(2), noon())
                .unwrap();
            let both = [original.clone(), reversal.clone()];
            let queries = std::iter::once(None).chain(tags.into_iter().map(Some));
            for tag in queries {
                for account in chart().accounts() {
                    let mut query = BalanceQuery::account(account.number());
                    if let Some(tag) = tag.clone() {
                        query = query.tagged(tag);
                    }
                    prop_assert_eq!(balance(&both, &chart(), &query), Ok(usd(0)));
                    let original_balance = balance([&original], &chart(), &query).unwrap();
                    prop_assert_eq!(
                        balance([&reversal], &chart(), &query),
                        original_balance.checked_neg().map_err(|_| BalanceError::Overflow)
                    );
                }
            }
        }
    }
}
