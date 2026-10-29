//! Plain rows as the tables hold them, and the one place they become domain
//! types. Loading parses every field and reports bad data as [`CorruptData`];
//! it never panics and never accepts something it can't explain.

use std::collections::BTreeMap;
use std::fmt;

use chrono::{DateTime, Utc};

use super::stored_tag::StoredTag;
use crate::account::{AccountNumber, Side};
use crate::idempotency::IdempotencyKey;
use crate::money::{Money, Usd};
use crate::transaction::{Entry, PostedTransaction, TransactionId};

/// A row of `transactions`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransactionRow {
    pub(crate) id: i64,
    pub(crate) idempotency_key: String,
    pub(crate) description: String,
    pub(crate) effective_at: DateTime<Utc>,
    pub(crate) recorded_at: DateTime<Utc>,
    pub(crate) reverses: Option<i64>,
}

/// A row of `entries`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EntryRow {
    pub(crate) transaction_id: i64,
    pub(crate) position: i16,
    pub(crate) account: i64,
    pub(crate) side: String,
    pub(crate) amount: i64,
    pub(crate) currency: String,
}

/// A row of `entry_tags`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TagRow {
    pub(crate) transaction_id: i64,
    pub(crate) position: i16,
    pub(crate) kind: String,
    pub(crate) value: String,
}

/// The currencies the tables may hold. Only USD in v1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CurrencyCode {
    Usd,
}

impl CurrencyCode {
    /// The code as stored, or `None` for a currency the ledger doesn't know.
    pub(crate) fn parse(code: &str) -> Option<Self> {
        match code {
            "USD" => Some(Self::Usd),
            _ => None,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Usd => "USD",
        }
    }
}

/// Why stored rows couldn't become a transaction. The database's constraints
/// should make each of these impossible, so any of them means the data was
/// changed behind the ledger's back. Every message names the transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CorruptData {
    /// An entry's currency isn't one the ledger knows.
    UnknownCurrency { transaction: i64, code: String },
    /// An entry's side is neither `debit` nor `credit`.
    UnknownSide { transaction: i64, side: String },
    /// A tag's kind is unknown to the tag type, or its value doesn't parse.
    UnknownTag {
        transaction: i64,
        kind: String,
        value: String,
    },
    /// A number is outside what its domain type allows. `column` names it:
    /// `id`, `reverses`, `account`, `amount`, or `tag position` (a tag whose
    /// position has no entry).
    OutOfRange {
        transaction: i64,
        column: &'static str,
        value: i64,
    },
    /// The stored idempotency key isn't a valid key.
    InvalidIdempotencyKey { transaction: i64, key: String },
    /// Two entry rows share a position.
    DuplicatePosition { transaction: i64, position: i16 },
    /// Two tag rows share a position and kind.
    DuplicateTag {
        transaction: i64,
        position: i16,
        kind: String,
    },
}

impl fmt::Display for CorruptData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CorruptData::UnknownCurrency { transaction, code } => write!(
                f,
                "corrupt data in transaction {transaction}: unknown currency {code:?}"
            ),
            CorruptData::UnknownSide { transaction, side } => write!(
                f,
                "corrupt data in transaction {transaction}: unknown side {side:?}"
            ),
            CorruptData::UnknownTag {
                transaction,
                kind,
                value,
            } => write!(
                f,
                "corrupt data in transaction {transaction}: tag {kind:?} with value {value:?} is not a known tag"
            ),
            CorruptData::OutOfRange {
                transaction,
                column,
                value,
            } => write!(
                f,
                "corrupt data in transaction {transaction}: {column} {value} is out of range"
            ),
            CorruptData::InvalidIdempotencyKey { transaction, key } => write!(
                f,
                "corrupt data in transaction {transaction}: stored idempotency key {key:?} is not valid"
            ),
            CorruptData::DuplicatePosition {
                transaction,
                position,
            } => write!(
                f,
                "corrupt data in transaction {transaction}: more than one entry at position {position}"
            ),
            CorruptData::DuplicateTag {
                transaction,
                position,
                kind,
            } => write!(
                f,
                "corrupt data in transaction {transaction}: more than one {kind:?} tag on the entry at position {position}"
            ),
        }
    }
}

impl std::error::Error for CorruptData {}

fn id(transaction: i64, column: &'static str, value: i64) -> Result<TransactionId, CorruptData> {
    u64::try_from(value)
        .map(TransactionId::new)
        .map_err(|_| CorruptData::OutOfRange {
            transaction,
            column,
            value,
        })
}

/// Turns one transaction's rows into a posted transaction. Entries come back
/// ordered by `position`, and each tag goes to the entry at its position.
/// This parses; it doesn't re-run the builder's rules.
pub(crate) fn assemble<T: StoredTag>(
    transaction: TransactionRow,
    entries: Vec<EntryRow>,
    tags: Vec<TagRow>,
) -> Result<PostedTransaction<Usd, T>, CorruptData> {
    let tx = transaction.id;
    let posted_id = id(tx, "id", tx)?;
    let reverses = transaction
        .reverses
        .map(|original| id(tx, "reverses", original))
        .transpose()?;
    let key = IdempotencyKey::new(transaction.idempotency_key.as_str()).map_err(|_| {
        CorruptData::InvalidIdempotencyKey {
            transaction: tx,
            key: transaction.idempotency_key.clone(),
        }
    })?;

    let mut parsed: BTreeMap<i16, (AccountNumber, Side, Money<Usd>, Vec<T>)> = BTreeMap::new();
    for row in entries {
        if CurrencyCode::parse(&row.currency).is_none() {
            return Err(CorruptData::UnknownCurrency {
                transaction: tx,
                code: row.currency,
            });
        }
        let side = match row.side.as_str() {
            "debit" => Side::Debit,
            "credit" => Side::Credit,
            _ => {
                return Err(CorruptData::UnknownSide {
                    transaction: tx,
                    side: row.side,
                });
            }
        };
        let account = u32::try_from(row.account)
            .map(AccountNumber::new)
            .map_err(|_| CorruptData::OutOfRange {
                transaction: tx,
                column: "account",
                value: row.account,
            })?;
        if row.amount <= 0 {
            return Err(CorruptData::OutOfRange {
                transaction: tx,
                column: "amount",
                value: row.amount,
            });
        }
        let amount = Money::from_minor_units(row.amount);
        if parsed
            .insert(row.position, (account, side, amount, Vec::new()))
            .is_some()
        {
            return Err(CorruptData::DuplicatePosition {
                transaction: tx,
                position: row.position,
            });
        }
    }

    let mut tags = tags;
    tags.sort_by(|a, b| (a.position, &a.kind).cmp(&(b.position, &b.kind)));
    if let Some(pair) = tags
        .windows(2)
        .find(|pair| (pair[0].position, &pair[0].kind) == (pair[1].position, &pair[1].kind))
    {
        return Err(CorruptData::DuplicateTag {
            transaction: tx,
            position: pair[0].position,
            kind: pair[0].kind.clone(),
        });
    }
    for row in tags {
        let Some((_, _, _, entry_tags)) = parsed.get_mut(&row.position) else {
            return Err(CorruptData::OutOfRange {
                transaction: tx,
                column: "tag position",
                value: i64::from(row.position),
            });
        };
        let Some(tag) = T::from_stored(&row.kind, &row.value) else {
            return Err(CorruptData::UnknownTag {
                transaction: tx,
                kind: row.kind,
                value: row.value,
            });
        };
        entry_tags.push(tag);
    }

    Ok(PostedTransaction::from_stored(
        posted_id,
        key,
        transaction.effective_at,
        transaction.description,
        reverses,
        parsed
            .into_values()
            .map(|(account, side, amount, tags)| Entry::from_stored(account, side, amount, tags))
            .collect(),
        transaction.recorded_at,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{AccountNumber, Side};
    use crate::test_support::{TestTag, noon};

    fn tx_row() -> TransactionRow {
        TransactionRow {
            id: 7,
            idempotency_key: "sale-1".to_string(),
            description: "A sale".to_string(),
            effective_at: noon(),
            recorded_at: noon(),
            reverses: None,
        }
    }

    fn entry_row(position: i16, account: i64, side: &str, amount: i64) -> EntryRow {
        EntryRow {
            transaction_id: 7,
            position,
            account,
            side: side.to_string(),
            amount,
            currency: "USD".to_string(),
        }
    }

    fn tag_row(position: i16, kind: &str, value: &str) -> TagRow {
        TagRow {
            transaction_id: 7,
            position,
            kind: kind.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn assembles_a_three_entry_transaction() {
        let posted = assemble::<TestTag>(
            tx_row(),
            vec![
                entry_row(2, 3, "credit", 40),
                entry_row(0, 1, "debit", 100),
                entry_row(1, 2, "credit", 60),
            ],
            vec![tag_row(2, "beta", "9"), tag_row(0, "alpha", "5")],
        )
        .unwrap();
        assert_eq!(posted.id().get(), 7);
        assert_eq!(posted.idempotency_key().as_str(), "sale-1");
        assert_eq!(posted.description(), "A sale");
        assert_eq!(posted.reverses(), None);
        let entries = posted.entries();
        let accounts: Vec<u32> = entries.iter().map(|e| e.account().get()).collect();
        assert_eq!(accounts, [1, 2, 3]);
        assert_eq!(entries[0].side(), Side::Debit);
        assert_eq!(entries[0].amount().minor_units(), 100);
        assert_eq!(entries[1].side(), Side::Credit);
        assert_eq!(entries[0].tags(), [TestTag::Alpha(5)]);
        assert!(entries[1].tags().is_empty());
        assert_eq!(entries[2].tags(), [TestTag::Beta(9)]);
        assert_eq!(entries[0].account(), AccountNumber::new(1));
    }

    #[test]
    fn carries_reverses_through() {
        let mut row = tx_row();
        row.reverses = Some(3);
        let posted = assemble::<TestTag>(
            row,
            vec![entry_row(0, 1, "debit", 1), entry_row(1, 2, "credit", 1)],
            vec![],
        )
        .unwrap();
        assert_eq!(posted.reverses().unwrap().get(), 3);
    }

    fn one_entry_error(entry: EntryRow, tags: Vec<TagRow>) -> CorruptData {
        assemble::<TestTag>(tx_row(), vec![entry], tags).unwrap_err()
    }

    #[test]
    fn unknown_currency_is_corrupt() {
        let mut entry = entry_row(0, 1, "debit", 1);
        entry.currency = "EUR".to_string();
        assert_eq!(
            one_entry_error(entry, vec![]),
            CorruptData::UnknownCurrency {
                transaction: 7,
                code: "EUR".to_string()
            }
        );
    }

    #[test]
    fn unknown_side_is_corrupt() {
        assert_eq!(
            one_entry_error(entry_row(0, 1, "left", 1), vec![]),
            CorruptData::UnknownSide {
                transaction: 7,
                side: "left".to_string()
            }
        );
    }

    #[test]
    fn unknown_tag_kind_is_corrupt() {
        assert_eq!(
            one_entry_error(entry_row(0, 1, "debit", 1), vec![tag_row(0, "gamma", "1")]),
            CorruptData::UnknownTag {
                transaction: 7,
                kind: "gamma".to_string(),
                value: "1".to_string()
            }
        );
    }

    #[test]
    fn unparseable_tag_value_is_corrupt() {
        assert_eq!(
            one_entry_error(entry_row(0, 1, "debit", 1), vec![tag_row(0, "alpha", "x")]),
            CorruptData::UnknownTag {
                transaction: 7,
                kind: "alpha".to_string(),
                value: "x".to_string()
            }
        );
    }

    #[test]
    fn tag_without_an_entry_is_corrupt() {
        assert_eq!(
            one_entry_error(entry_row(0, 1, "debit", 1), vec![tag_row(4, "alpha", "1")]),
            CorruptData::OutOfRange {
                transaction: 7,
                column: "tag position",
                value: 4
            }
        );
    }

    #[test]
    fn negative_id_is_out_of_range() {
        let mut row = tx_row();
        row.id = -1;
        assert_eq!(
            assemble::<TestTag>(row, vec![], vec![]).unwrap_err(),
            CorruptData::OutOfRange {
                transaction: -1,
                column: "id",
                value: -1
            }
        );
    }

    #[test]
    fn negative_reverses_is_out_of_range() {
        let mut row = tx_row();
        row.reverses = Some(-2);
        assert_eq!(
            assemble::<TestTag>(row, vec![], vec![]).unwrap_err(),
            CorruptData::OutOfRange {
                transaction: 7,
                column: "reverses",
                value: -2
            }
        );
    }

    #[test]
    fn account_beyond_u32_is_out_of_range() {
        let beyond = i64::from(u32::MAX) + 1;
        assert_eq!(
            one_entry_error(entry_row(0, beyond, "debit", 1), vec![]),
            CorruptData::OutOfRange {
                transaction: 7,
                column: "account",
                value: beyond
            }
        );
        assert!(matches!(
            one_entry_error(entry_row(0, -1, "debit", 1), vec![]),
            CorruptData::OutOfRange {
                column: "account",
                ..
            }
        ));
    }

    #[test]
    fn non_positive_amount_is_out_of_range() {
        for amount in [0, -5] {
            assert_eq!(
                one_entry_error(entry_row(0, 1, "debit", amount), vec![]),
                CorruptData::OutOfRange {
                    transaction: 7,
                    column: "amount",
                    value: amount
                }
            );
        }
    }

    #[test]
    fn invalid_stored_key_is_corrupt() {
        let mut row = tx_row();
        row.idempotency_key = " padded ".to_string();
        assert_eq!(
            assemble::<TestTag>(row, vec![], vec![]).unwrap_err(),
            CorruptData::InvalidIdempotencyKey {
                transaction: 7,
                key: " padded ".to_string()
            }
        );
    }

    #[test]
    fn duplicate_entry_position_is_corrupt() {
        let error = assemble::<TestTag>(
            tx_row(),
            vec![
                entry_row(0, 1, "debit", 100),
                entry_row(1, 2, "credit", 100),
                entry_row(1, 3, "credit", 5),
            ],
            vec![],
        )
        .unwrap_err();
        assert_eq!(
            error,
            CorruptData::DuplicatePosition {
                transaction: 7,
                position: 1
            }
        );
    }

    #[test]
    fn duplicate_tag_is_corrupt() {
        let error = assemble::<TestTag>(
            tx_row(),
            vec![entry_row(0, 1, "debit", 1)],
            vec![tag_row(0, "alpha", "1"), tag_row(0, "alpha", "2")],
        )
        .unwrap_err();
        assert_eq!(
            error,
            CorruptData::DuplicateTag {
                transaction: 7,
                position: 0,
                kind: "alpha".to_string()
            }
        );
    }

    #[test]
    fn display_names_the_transaction() {
        let message = CorruptData::UnknownSide {
            transaction: 7,
            side: "left".to_string(),
        }
        .to_string();
        assert!(message.contains("transaction 7"), "{message}");
        assert!(message.contains("left"), "{message}");
    }
}
