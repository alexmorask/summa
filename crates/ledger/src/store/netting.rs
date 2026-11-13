//! Netting a batch of entries in memory: the pure half of the balance
//! worker. It adds each entry to the stored debit and credit totals of its
//! account and of each of its tags, so the database is written once per
//! changed total instead of once per entry.

use std::collections::hash_map::Entry;
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
    pub(crate) const ZERO: Self = Self::from_minor_units(0, 0);

    /// Totals from stored whole-cent debit and credit amounts.
    pub(crate) const fn from_minor_units(debit: i64, credit: i64) -> Self {
        Self {
            debit: Money::from_minor_units(debit),
            credit: Money::from_minor_units(credit),
        }
    }

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

/// Every key the batch's entries add to.
pub(crate) fn keys_for(transactions: &[LogTransaction]) -> HashSet<BalanceKey> {
    transactions
        .iter()
        .flat_map(|t| &t.entries)
        .flat_map(entry_keys)
        .collect()
}

/// The account key of `entry`, then one key per tag.
fn entry_keys(entry: &LogEntry) -> impl Iterator<Item = BalanceKey> + '_ {
    std::iter::once(BalanceKey::Account(entry.account)).chain(
        entry
            .tags
            .iter()
            .map(|(kind, value)| BalanceKey::Tag(entry.account, kind.clone(), value.clone())),
    )
}

/// Adds every entry of `transactions`, in order, to the `stored` totals and
/// returns only the keys the batch touched, with their new totals. A key
/// missing from `stored` starts at zero. If a total would overflow, it
/// returns only the error, naming the first transaction that overflows.
pub(crate) fn net(
    stored: impl IntoIterator<Item = (BalanceKey, Totals)>,
    transactions: &[LogTransaction],
) -> Result<Vec<(BalanceKey, Totals)>, NettingOverflow> {
    let mut stored: HashMap<BalanceKey, Totals> = stored.into_iter().collect();
    let mut touched: HashMap<BalanceKey, Totals> = HashMap::new();
    for transaction in transactions {
        for entry in &transaction.entries {
            for key in entry_keys(entry) {
                let totals = match touched.entry(key) {
                    Entry::Occupied(occupied) => occupied.into_mut(),
                    Entry::Vacant(vacant) => {
                        let start = stored.remove(vacant.key()).unwrap_or(Totals::ZERO);
                        vacant.insert(start)
                    }
                };
                let side = match entry.side {
                    Side::Debit => &mut totals.debit,
                    Side::Credit => &mut totals.credit,
                };
                *side = side
                    .checked_add(entry.amount)
                    .map_err(|MoneyError::Overflow| NettingOverflow {
                        transaction: transaction.id,
                    })?;
            }
        }
    }
    Ok(touched.into_iter().collect())
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
        Totals::from_minor_units(debit, credit)
    }

    fn net_map(
        stored: impl IntoIterator<Item = (BalanceKey, Totals)>,
        transactions: &[LogTransaction],
    ) -> HashMap<BalanceKey, Totals> {
        net(stored, transactions).unwrap().into_iter().collect()
    }

    // Stored before the batch, in cents (debit, credit): 2000 (0, 500,000),
    // 4000 (0, 100,000), 1010 (30,000, 0), 2100 (0, 5,000).
    // 501: debit 2000 2,000, credit 4000 2,000.
    // 502: debit 2000 1,000, credit 4000 1,000.
    // 503: debit 1010 2,160, credit 2000 2,000, credit 2100 160.
    // After: 2000 (3,000, 502,000) nets to a 499,000 credit; 4000 (0, 103,000);
    // 1010 (32,160, 0); 2100 (0, 5,160). Seven entries, four changed totals.
    #[test]
    fn batch_of_three_transactions_nets_to_four_changes() {
        let stored = [
            (BalanceKey::Account(acct(2000)), totals(0, 500_000)),
            (BalanceKey::Account(acct(4000)), totals(0, 100_000)),
            (BalanceKey::Account(acct(1010)), totals(30_000, 0)),
            (BalanceKey::Account(acct(2100)), totals(0, 5_000)),
        ];
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
        let updates = net_map(stored, &batch);
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
        let transaction = LogTransaction {
            id: 1,
            entries: vec![entry(1, Side::Debit, 500), entry(1, Side::Credit, 500)],
        };
        let updates = net_map([], &[transaction]);
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
        let updates = net_map([], &[batch]);
        let tag_key = |value: &str| BalanceKey::Tag(acct(1), "k".to_string(), value.to_string());
        assert_eq!(updates.len(), 4);
        assert_eq!(updates[&tag_key("a:b")], totals(10, 0));
        assert_eq!(updates[&tag_key("")], totals(10, 0));
        assert_eq!(updates[&tag_key("é")], totals(20, 0));
        assert_eq!(updates[&BalanceKey::Account(acct(1))], totals(40, 0));
    }

    #[test]
    fn untagged_entries_add_only_to_the_account_balance() {
        let transaction = LogTransaction {
            id: 1,
            entries: vec![entry(1, Side::Debit, 5), entry(2, Side::Credit, 5)],
        };
        assert!(
            net([], &[transaction])
                .unwrap()
                .iter()
                .all(|(key, _)| matches!(key, BalanceKey::Account(_)))
        );
    }

    #[test]
    fn overflow_names_the_transaction() {
        let stored = [(BalanceKey::Account(acct(1)), totals(i64::MAX - 5, 0))];
        let transaction = LogTransaction {
            id: 7,
            entries: vec![entry(1, Side::Debit, 10)],
        };
        assert_eq!(
            net(stored, &[transaction]),
            Err(NettingOverflow { transaction: 7 })
        );
    }

    #[test]
    fn overflow_after_earlier_transactions_returns_only_the_error() {
        let stored = [(BalanceKey::Account(acct(1)), totals(i64::MAX - 15, 0))];
        let batch = [
            LogTransaction {
                id: 6,
                entries: vec![entry(1, Side::Debit, 10), entry(2, Side::Credit, 10)],
            },
            LogTransaction {
                id: 7,
                entries: vec![entry(1, Side::Debit, 10), entry(2, Side::Credit, 10)],
            },
        ];
        assert_eq!(net(stored, &batch), Err(NettingOverflow { transaction: 7 }));
    }

    #[test]
    fn updates_hold_only_touched_keys() {
        let stored = [
            (BalanceKey::Account(acct(1)), totals(1, 0)),
            (BalanceKey::Account(acct(9)), totals(9, 9)),
        ];
        let transaction = LogTransaction {
            id: 1,
            entries: vec![entry(1, Side::Debit, 1)],
        };
        let updates = net_map(stored, &[transaction]);
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
            keys_for(&batch),
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
            let log: Vec<LogTransaction> = transactions
                .iter()
                .map(|transaction| LogTransaction {
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
                })
                .collect();
            let updates = net_map([], &log);
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
