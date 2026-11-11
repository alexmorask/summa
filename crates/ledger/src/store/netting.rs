//! Netting a batch of entries in memory: the pure half of the balance
//! worker. It adds each entry to the stored debit and credit totals of its
//! account and of each of its tags, so the database is written once per
//! changed total instead of once per entry.

use std::collections::{HashMap, HashSet};

use crate::account::{AccountNumber, Side};
use crate::money::{Money, MoneyError, Usd};

/// The running debit and credit totals of one account or one account-and-tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Totals {
    pub(crate) debit: Money<Usd>,
    pub(crate) credit: Money<Usd>,
}

impl Totals {
    pub(crate) const ZERO: Self = Self {
        debit: Money::from_minor_units(0),
        credit: Money::from_minor_units(0),
    };

    /// The normal side's total minus the other side's, the same definition
    /// as `balance` without cutoffs.
    pub(crate) fn net(self, normal: Side) -> Result<Money<Usd>, MoneyError> {
        match normal {
            Side::Debit => self.debit.checked_sub(self.credit),
            Side::Credit => self.credit.checked_sub(self.debit),
        }
    }
}

/// What a stored total belongs to. A tag is its stored kind and value text.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BalanceKey {
    Account(AccountNumber),
    Tag(AccountNumber, String, String),
}

/// An entry as read from the log: tags are (kind, value) text.
pub(crate) struct LogEntry {
    pub(crate) account: AccountNumber,
    pub(crate) side: Side,
    pub(crate) amount: Money<Usd>,
    pub(crate) tags: Vec<(String, String)>,
}

pub(crate) struct LogTransaction {
    pub(crate) id: i64,
    pub(crate) entries: Vec<LogEntry>,
}

/// A total would not fit in an `i64`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct NettingOverflow {
    pub(crate) transaction: i64,
}

/// Stored totals for the keys a batch touches, updated as the batch is applied.
pub(crate) struct Netting {
    /// Each total, and whether the batch has changed it.
    totals: HashMap<BalanceKey, (Totals, bool)>,
}

impl Netting {
    /// Every key the batch's entries add to.
    pub(crate) fn keys_for(transactions: &[LogTransaction]) -> HashSet<BalanceKey> {
        let mut keys = HashSet::new();
        for entry in transactions.iter().flat_map(|t| &t.entries) {
            keys.extend(Self::entry_keys(entry));
        }
        keys
    }

    fn entry_keys(entry: &LogEntry) -> impl Iterator<Item = BalanceKey> + '_ {
        std::iter::once(BalanceKey::Account(entry.account)).chain(
            entry
                .tags
                .iter()
                .map(|(kind, value)| BalanceKey::Tag(entry.account, kind.clone(), value.clone())),
        )
    }

    /// Starts from the stored totals. A key not given starts at zero.
    pub(crate) fn new(stored: impl IntoIterator<Item = (BalanceKey, Totals)>) -> Self {
        Self {
            totals: stored
                .into_iter()
                .map(|(key, totals)| (key, (totals, false)))
                .collect(),
        }
    }

    /// Adds every entry of `transaction`. On overflow the value is unusable
    /// and the caller abandons the batch.
    pub(crate) fn apply(&mut self, transaction: &LogTransaction) -> Result<(), NettingOverflow> {
        for entry in &transaction.entries {
            for key in Self::entry_keys(entry) {
                let (totals, touched) = self.totals.entry(key).or_insert((Totals::ZERO, false));
                let side = match entry.side {
                    Side::Debit => &mut totals.debit,
                    Side::Credit => &mut totals.credit,
                };
                *side = side
                    .checked_add(entry.amount)
                    .map_err(|MoneyError::Overflow| NettingOverflow {
                        transaction: transaction.id,
                    })?;
                *touched = true;
            }
        }
        Ok(())
    }

    /// The touched keys with their new totals.
    pub(crate) fn updates(self) -> Vec<(BalanceKey, Totals)> {
        self.totals
            .into_iter()
            .filter(|(_, (_, touched))| *touched)
            .map(|(key, (totals, _))| (key, totals))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::balance::{BalanceQuery, balance};
    use crate::store::StoredTag;
    use crate::tag::Tag;
    use crate::test_support::{any_test_tag, chart, posted_transactions, usd};

    const fn acct(n: u32) -> AccountNumber {
        AccountNumber::new(n)
    }

    fn entry(account: u32, side: Side, cents: i64) -> LogEntry {
        LogEntry {
            account: acct(account),
            side,
            amount: usd(cents),
            tags: vec![],
        }
    }

    fn totals(debit: i64, credit: i64) -> Totals {
        Totals {
            debit: usd(debit),
            credit: usd(credit),
        }
    }

    fn updates_map(netting: Netting) -> HashMap<BalanceKey, Totals> {
        netting.updates().into_iter().collect()
    }

    #[test]
    fn design_doc_batch_nets_to_four_changes() {
        let mut netting = Netting::new([
            (BalanceKey::Account(acct(2000)), totals(0, 500_000)),
            (BalanceKey::Account(acct(4000)), totals(0, 100_000)),
            (BalanceKey::Account(acct(1010)), totals(30_000, 0)),
            (BalanceKey::Account(acct(2100)), totals(0, 5_000)),
        ]);
        let batch = [
            LogTransaction {
                id: 501,
                entries: vec![
                    entry(2000, Side::Debit, 2_000),
                    entry(4000, Side::Credit, 2_000),
                ],
            },
            LogTransaction {
                id: 502,
                entries: vec![
                    entry(2000, Side::Debit, 1_000),
                    entry(4000, Side::Credit, 1_000),
                ],
            },
            LogTransaction {
                id: 503,
                entries: vec![
                    entry(1010, Side::Debit, 2_160),
                    entry(2000, Side::Credit, 2_000),
                    entry(2100, Side::Credit, 160),
                ],
            },
        ];
        for transaction in &batch {
            netting.apply(transaction).unwrap();
        }
        let updates = updates_map(netting);
        assert_eq!(updates.len(), 4);
        let liability = updates[&BalanceKey::Account(acct(2000))];
        assert_eq!(liability, totals(3_000, 502_000));
        assert_eq!(liability.net(Side::Credit), Ok(usd(499_000)));
        assert_eq!(
            updates[&BalanceKey::Account(acct(4000))],
            totals(0, 103_000)
        );
        assert_eq!(updates[&BalanceKey::Account(acct(1010))], totals(32_160, 0));
        assert_eq!(updates[&BalanceKey::Account(acct(2100))], totals(0, 5_160));
    }

    #[test]
    fn same_account_on_both_sides_grows_both_totals() {
        let mut netting = Netting::new([]);
        let transaction = LogTransaction {
            id: 1,
            entries: vec![entry(1, Side::Debit, 500), entry(1, Side::Credit, 500)],
        };
        netting.apply(&transaction).unwrap();
        let updates = updates_map(netting);
        let account = updates[&BalanceKey::Account(acct(1))];
        assert_eq!(account, totals(500, 500));
        assert_eq!(account.net(Side::Debit), Ok(usd(0)));
    }

    #[test]
    fn tag_balances_key_on_kind_and_value_text() {
        let tagged = |value: &str| LogEntry {
            tags: vec![("k".to_string(), value.to_string())],
            ..entry(1, Side::Debit, 10)
        };
        let batch = LogTransaction {
            id: 1,
            entries: vec![tagged("a:b"), tagged(""), tagged("é"), tagged("é")],
        };
        let mut netting = Netting::new([]);
        netting.apply(&batch).unwrap();
        let updates = updates_map(netting);
        let tag_key = |value: &str| BalanceKey::Tag(acct(1), "k".to_string(), value.to_string());
        assert_eq!(updates.len(), 4);
        assert_eq!(updates[&tag_key("a:b")], totals(10, 0));
        assert_eq!(updates[&tag_key("")], totals(10, 0));
        assert_eq!(updates[&tag_key("é")], totals(20, 0));
        assert_eq!(updates[&BalanceKey::Account(acct(1))], totals(40, 0));
    }

    #[test]
    fn untagged_entries_add_only_to_the_account_balance() {
        let mut netting = Netting::new([]);
        let transaction = LogTransaction {
            id: 1,
            entries: vec![entry(1, Side::Debit, 5), entry(2, Side::Credit, 5)],
        };
        netting.apply(&transaction).unwrap();
        assert!(
            netting
                .updates()
                .iter()
                .all(|(key, _)| matches!(key, BalanceKey::Account(_)))
        );
    }

    #[test]
    fn overflow_names_the_transaction() {
        let mut netting = Netting::new([(
            BalanceKey::Account(acct(1)),
            Totals {
                debit: usd(i64::MAX - 5),
                credit: usd(0),
            },
        )]);
        let transaction = LogTransaction {
            id: 7,
            entries: vec![entry(1, Side::Debit, 10)],
        };
        assert_eq!(
            netting.apply(&transaction),
            Err(NettingOverflow { transaction: 7 })
        );
    }

    #[test]
    fn updates_hold_only_touched_keys() {
        let mut netting = Netting::new([
            (BalanceKey::Account(acct(1)), totals(1, 0)),
            (BalanceKey::Account(acct(9)), totals(9, 9)),
        ]);
        let transaction = LogTransaction {
            id: 1,
            entries: vec![entry(1, Side::Debit, 1)],
        };
        netting.apply(&transaction).unwrap();
        let updates = updates_map(netting);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[&BalanceKey::Account(acct(1))], totals(2, 0));
    }

    #[test]
    fn keys_for_lists_account_and_tag_keys() {
        let batch = [LogTransaction {
            id: 1,
            entries: vec![LogEntry {
                tags: vec![("k".to_string(), "v".to_string())],
                ..entry(1, Side::Debit, 1)
            }],
        }];
        assert_eq!(
            Netting::keys_for(&batch),
            HashSet::from([
                BalanceKey::Account(acct(1)),
                BalanceKey::Tag(acct(1), "k".to_string(), "v".to_string()),
            ])
        );
    }

    #[test]
    fn net_uses_the_normal_side() {
        let t = totals(100, 30);
        assert_eq!(t.net(Side::Debit), Ok(usd(70)));
        assert_eq!(t.net(Side::Credit), Ok(usd(-70)));
    }

    proptest! {
        #[test]
        fn netting_matches_balance_for_every_account_and_tag(
            transactions in posted_transactions(),
            tag in prop::option::of(any_test_tag()),
        ) {
            let mut netting = Netting::new([]);
            for transaction in &transactions {
                let log = LogTransaction {
                    id: transaction.id().get() as i64,
                    entries: transaction
                        .entries()
                        .iter()
                        .map(|e| LogEntry {
                            account: e.account(),
                            side: e.side(),
                            amount: e.amount(),
                            tags: e
                                .tags()
                                .iter()
                                .map(|t| (t.kind().to_string(), t.value()))
                                .collect(),
                        })
                        .collect(),
                };
                netting.apply(&log).unwrap();
            }
            let updates = updates_map(netting);
            for number in 1..=4 {
                let account = acct(number);
                let normal = chart().get(account).unwrap().normal_side();
                let (key, query) = match &tag {
                    None => (BalanceKey::Account(account), BalanceQuery::account(account)),
                    Some(t) => (
                        BalanceKey::Tag(account, t.kind().to_string(), t.value()),
                        BalanceQuery::account(account).tagged(t.clone()),
                    ),
                };
                let netted = updates
                    .get(&key)
                    .copied()
                    .unwrap_or(Totals::ZERO)
                    .net(normal)
                    .unwrap();
                let expected = balance(&transactions, &chart(), &query).unwrap();
                prop_assert_eq!(netted, expected);
            }
        }
    }
}
