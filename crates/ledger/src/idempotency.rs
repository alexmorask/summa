//! Idempotency keys: the caller's name for one intended transaction.
//!
//! A caller that isn't sure whether its request went through (a timeout, a
//! crash) retries with the same key. Storage can then tell a retry of the
//! same request from a different request that reused the key, by comparing
//! their contents with [`same_contents`].

use std::collections::HashSet;
use std::fmt;

use crate::money::Currency;
use crate::tag::Tag;
use crate::transaction::{Entry, PostedTransaction, UnpostedTransaction};

/// The caller-chosen name of one intended transaction. Two requests with the
/// same key are meant to be the same transaction, so posting the second one
/// must not record it again.
///
/// A key is 1 to 255 bytes with no whitespace at either end. Derive it from
/// the business event (`"renewal:sub-42:2026-11"`), not from a random value
/// made per attempt, or retries get different keys and double-post.
///
/// Keys are compared as exact bytes: `charge-1` and `Charge-1` are different
/// keys, and nothing is normalized. Callers must build keys consistently.
///
/// ```
/// use ledger::IdempotencyKey;
///
/// let key = IdempotencyKey::new("renewal:sub-42:2026-11").unwrap();
/// assert_eq!(key.as_str(), "renewal:sub-42:2026-11");
/// assert!(IdempotencyKey::new("").is_err());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// The longest key, in bytes (not characters).
    pub const MAX_LENGTH: usize = 255;

    /// Checks `key` and wraps it. In order:
    ///
    /// 1. It must not be empty.
    /// 2. It must not start or end with whitespace
    ///    ([`char::is_whitespace`]).
    /// 3. It must not contain a NUL (`'\0'`) character, which PostgreSQL
    ///    text can't hold.
    /// 4. It must be at most [`MAX_LENGTH`](Self::MAX_LENGTH) bytes.
    pub fn new(key: impl Into<String>) -> Result<Self, IdempotencyKeyError> {
        let key = key.into();
        if key.is_empty() {
            return Err(IdempotencyKeyError::Empty);
        }
        if key.starts_with(char::is_whitespace) || key.ends_with(char::is_whitespace) {
            return Err(IdempotencyKeyError::SurroundingWhitespace);
        }
        if key.contains('\0') {
            return Err(IdempotencyKeyError::ContainsNul);
        }
        if key.len() > Self::MAX_LENGTH {
            return Err(IdempotencyKeyError::TooLong { length: key.len() });
        }
        Ok(Self(key))
    }

    /// The key as text.
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why [`IdempotencyKey::new`] rejected a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdempotencyKeyError {
    /// The key is empty.
    Empty,
    /// The key is longer than [`IdempotencyKey::MAX_LENGTH`] bytes.
    TooLong { length: usize },
    /// The key starts or ends with whitespace.
    SurroundingWhitespace,
    /// The key contains a NUL (`'\0'`) character, which the database can't store.
    ContainsNul,
}

impl fmt::Display for IdempotencyKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdempotencyKeyError::Empty => write!(f, "the idempotency key is empty"),
            IdempotencyKeyError::TooLong { length } => write!(
                f,
                "the idempotency key is {length} bytes, over the limit of {}",
                IdempotencyKey::MAX_LENGTH
            ),
            IdempotencyKeyError::SurroundingWhitespace => {
                write!(f, "the idempotency key starts or ends with whitespace")
            }
            IdempotencyKeyError::ContainsNul => {
                write!(f, "the idempotency key contains a NUL byte")
            }
        }
    }
}

impl std::error::Error for IdempotencyKeyError {}

/// Whether a retry (`request`) asks for the same transaction as one already
/// posted (`existing`). It does when the description, effective time and
/// `reverses` link are equal and the entries match one for one. Two entries
/// match when account, side and amount are equal and the tags are equal as
/// sets, so entry order and tag order don't matter.
///
/// The key isn't compared (storage looks the existing transaction up by it),
/// and neither are the ID and recorded time, which only the ledger sets.
///
/// Each request entry, in order, takes the first unpaired partner. That is
/// safe because matching is exact equality, so no other pairing could do
/// better.
#[cfg_attr(
    not(any(test, feature = "postgres")),
    expect(dead_code, reason = "called by the PostgreSQL store")
)]
pub(crate) fn same_contents<C: Currency, T: Tag>(
    request: &UnpostedTransaction<C, T>,
    existing: &PostedTransaction<C, T>,
) -> bool {
    if request.description() != existing.description()
        || request.effective_at() != existing.effective_at()
        || request.reverses() != existing.reverses()
        || request.entries().len() != existing.entries().len()
    {
        return false;
    }
    let mut paired = vec![false; existing.entries().len()];
    request.entries().iter().all(|entry| {
        let partner = existing
            .entries()
            .iter()
            .enumerate()
            .find(|&(i, candidate)| !paired[i] && matches(entry, candidate));
        match partner {
            Some((i, _)) => {
                paired[i] = true;
                true
            }
            None => false,
        }
    })
}

/// Whether two entries are equal, comparing tags as sets. An entry can't hold
/// two tags of one kind, so a set of tags is a faithful way to compare them.
fn matches<C: Currency, T: Tag>(a: &Entry<C, T>, b: &Entry<C, T>) -> bool {
    a.account() == b.account()
        && a.side() == b.side()
        && a.amount() == b.amount()
        && a.tags().iter().collect::<HashSet<_>>() == b.tags().iter().collect::<HashSet<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::Side;
    use crate::money::Usd;
    use crate::test_support::*;
    use crate::transaction::TransactionId;
    use chrono::{DateTime, TimeDelta, Utc};
    use proptest::prelude::*;

    type Posted = PostedTransaction<Usd, TestTag>;
    type Unposted = UnpostedTransaction<Usd, TestTag>;

    // --- The key ---

    #[test]
    fn accepts_a_derived_key() {
        let key = IdempotencyKey::new("renewal:sub-42:2026-11").unwrap();
        assert_eq!(key.as_str(), "renewal:sub-42:2026-11");
        assert_eq!(key.to_string(), "renewal:sub-42:2026-11");
    }

    #[test]
    fn rejects_empty_key() {
        assert_eq!(IdempotencyKey::new(""), Err(IdempotencyKeyError::Empty));
    }

    #[test]
    fn rejects_key_over_255_bytes() {
        assert_eq!(
            IdempotencyKey::new("a".repeat(256)),
            Err(IdempotencyKeyError::TooLong { length: 256 })
        );
        assert!(IdempotencyKey::new("a".repeat(255)).is_ok());
        // 128 characters, but 2 bytes each.
        assert_eq!(
            IdempotencyKey::new("é".repeat(128)),
            Err(IdempotencyKeyError::TooLong { length: 256 })
        );
        assert!(IdempotencyKey::new("é".repeat(127)).is_ok());
    }

    #[test]
    fn rejects_surrounding_whitespace() {
        for text in [" a", "a ", "a\n", "\ta", " "] {
            assert_eq!(
                IdempotencyKey::new(text),
                Err(IdempotencyKeyError::SurroundingWhitespace),
                "{text:?}"
            );
        }
        assert!(IdempotencyKey::new("a b").is_ok());
    }

    #[test]
    fn key_with_nul_is_rejected() {
        for text in ["a\0b", "\0", "a\0"] {
            assert_eq!(
                IdempotencyKey::new(text),
                Err(IdempotencyKeyError::ContainsNul),
                "{text:?}"
            );
        }
        assert_eq!(
            IdempotencyKeyError::ContainsNul.to_string(),
            "the idempotency key contains a NUL byte"
        );
    }

    #[test]
    fn checks_emptiness_before_whitespace_before_nul_before_length() {
        assert_eq!(IdempotencyKey::new(""), Err(IdempotencyKeyError::Empty));
        assert_eq!(
            IdempotencyKey::new(" \0"),
            Err(IdempotencyKeyError::SurroundingWhitespace)
        );
        assert_eq!(
            IdempotencyKey::new(format!("{}\0", "a".repeat(300))),
            Err(IdempotencyKeyError::ContainsNul)
        );
        assert_eq!(
            IdempotencyKey::new(format!("{} ", "a".repeat(300))),
            Err(IdempotencyKeyError::SurroundingWhitespace)
        );
    }

    #[test]
    fn keys_are_case_sensitive() {
        assert_ne!(key("charge-1"), key("Charge-1"));
        assert_eq!(key("charge-1"), key("charge-1"));
    }

    #[test]
    fn key_errors_explain_themselves() {
        assert_eq!(
            IdempotencyKeyError::Empty.to_string(),
            "the idempotency key is empty"
        );
        assert_eq!(
            IdempotencyKeyError::TooLong { length: 256 }.to_string(),
            "the idempotency key is 256 bytes, over the limit of 255"
        );
        assert_eq!(
            IdempotencyKeyError::SurroundingWhitespace.to_string(),
            "the idempotency key starts or ends with whitespace"
        );
    }

    // --- Same contents ---

    fn entries() -> Vec<TestEntry> {
        vec![
            (ASSET, Side::Debit, 2_160, vec![TestTag::Alpha(42)]),
            (
                LIABILITY,
                Side::Credit,
                2_000,
                vec![TestTag::Alpha(42), TestTag::Beta(9)],
            ),
            (REVENUE, Side::Credit, 160, vec![TestTag::Alpha(42)]),
        ]
    }

    fn request_with(
        entries: &[TestEntry],
        reverses: Option<u64>,
        description: &str,
        effective_at: DateTime<Utc>,
    ) -> Unposted {
        let mut builder = UnpostedTransaction::builder(key("k"), effective_at, description);
        if let Some(original) = reverses {
            builder = builder.reversing(TransactionId::new(original));
        }
        entries
            .iter()
            .cloned()
            .fold(builder, |b, (account, side, cents, tags)| match side {
                Side::Debit => b.debit(account, usd(cents), tags),
                Side::Credit => b.credit(account, usd(cents), tags),
            })
            .build(&chart(), noon())
            .unwrap()
    }

    fn request(entries: &[TestEntry]) -> Unposted {
        request_with(entries, None, "Sale", noon())
    }

    fn existing(entries: &[TestEntry]) -> Posted {
        request(entries)
            .post(
                &chart(),
                TransactionId::new(7),
                noon() + TimeDelta::hours(1),
            )
            .unwrap()
    }

    #[test]
    fn same_contents_true_for_an_identical_request() {
        assert!(same_contents(&request(&entries()), &existing(&entries())));
    }

    #[test]
    fn same_contents_ignores_the_key_the_id_and_the_recorded_time() {
        let other_key = UnpostedTransaction::<Usd, TestTag>::builder(key("other"), noon(), "Sale")
            .debit(ASSET, usd(5), [])
            .credit(REVENUE, usd(5), [])
            .build(&chart(), noon())
            .unwrap();
        let same_but_key = request_with(
            &[
                (ASSET, Side::Debit, 5, vec![]),
                (REVENUE, Side::Credit, 5, vec![]),
            ],
            None,
            "Sale",
            noon(),
        );
        let posted = same_but_key
            .post(
                &chart(),
                TransactionId::new(999),
                noon() + TimeDelta::days(9),
            )
            .unwrap();
        assert!(same_contents(&other_key, &posted));
    }

    #[test]
    fn same_contents_true_when_entries_are_reordered() {
        let mut reordered = entries();
        reordered.reverse();
        assert!(same_contents(&request(&reordered), &existing(&entries())));
    }

    #[test]
    fn same_contents_true_when_an_entrys_tags_are_reordered() {
        let mut reordered = entries();
        reordered[1].3.reverse();
        assert!(same_contents(&request(&reordered), &existing(&entries())));
    }

    #[test]
    fn same_contents_false_when_the_description_differs() {
        let other = request_with(&entries(), None, "Other", noon());
        assert!(!same_contents(&other, &existing(&entries())));
    }

    #[test]
    fn same_contents_false_when_the_effective_time_differs() {
        let other = request_with(&entries(), None, "Sale", noon() - TimeDelta::seconds(1));
        assert!(!same_contents(&other, &existing(&entries())));
    }

    #[test]
    fn same_contents_false_when_reverses_differs() {
        let linked = request_with(&entries(), Some(3), "Sale", noon());
        assert!(!same_contents(&linked, &existing(&entries())));
        let linked_posted = linked
            .clone()
            .post(&chart(), TransactionId::new(7), noon())
            .unwrap();
        assert!(same_contents(&linked, &linked_posted));
        let other_link = request_with(&entries(), Some(4), "Sale", noon());
        assert!(!same_contents(&other_link, &linked_posted));
        assert!(!same_contents(&request(&entries()), &linked_posted));
    }

    #[test]
    fn same_contents_false_when_an_amount_differs() {
        let mut other = entries();
        other[1].2 = 1_999;
        other[2].2 = 161;
        assert!(!same_contents(&request(&other), &existing(&entries())));
    }

    #[test]
    fn same_contents_false_when_a_side_differs() {
        let mixed = vec![
            (ASSET, Side::Debit, 100, vec![]),
            (REVENUE, Side::Credit, 100, vec![]),
        ];
        let flipped = vec![
            (ASSET, Side::Credit, 100, vec![]),
            (REVENUE, Side::Debit, 100, vec![]),
        ];
        assert!(!same_contents(&request(&flipped), &existing(&mixed)));
    }

    #[test]
    fn same_contents_false_when_an_account_differs() {
        let mut other = entries();
        other[2].0 = CONTRA_REVENUE;
        other[2].1 = Side::Credit;
        assert!(!same_contents(&request(&other), &existing(&entries())));
    }

    #[test]
    fn same_contents_false_when_a_tag_differs() {
        let mut other = entries();
        other[0].3 = vec![TestTag::Alpha(43)];
        assert!(!same_contents(&request(&other), &existing(&entries())));
        let mut fewer = entries();
        fewer[1].3 = vec![TestTag::Alpha(42)];
        assert!(!same_contents(&request(&fewer), &existing(&entries())));
    }

    #[test]
    fn same_contents_false_when_the_entry_counts_differ() {
        let split = vec![
            (ASSET, Side::Debit, 100, vec![]),
            (REVENUE, Side::Credit, 60, vec![]),
            (REVENUE, Side::Credit, 40, vec![]),
        ];
        let whole = vec![
            (ASSET, Side::Debit, 100, vec![]),
            (REVENUE, Side::Credit, 100, vec![]),
        ];
        assert!(!same_contents(&request(&split), &existing(&whole)));
        assert!(!same_contents(&request(&whole), &existing(&split)));
    }

    #[test]
    fn same_contents_pairs_duplicate_entries_as_a_multiset() {
        let twice = vec![
            (ASSET, Side::Debit, 100, vec![]),
            (ASSET, Side::Debit, 100, vec![]),
            (REVENUE, Side::Credit, 200, vec![]),
        ];
        let mixed = vec![
            (ASSET, Side::Debit, 100, vec![]),
            (ASSET, Side::Debit, 50, vec![]),
            (ASSET, Side::Debit, 50, vec![]),
            (REVENUE, Side::Credit, 200, vec![]),
        ];
        assert!(same_contents(&request(&twice), &existing(&twice)));
        assert!(!same_contents(&request(&twice), &existing(&mixed)));
    }

    proptest! {
        #[test]
        fn posting_preserves_same_contents(
            entries in valid_entries(),
            earlier in 0_i64..=30 * 86_400,
            later in 0_i64..=5 * 86_400,
            id in 1_u64..1_000,
        ) {
            let effective_at = noon() - TimeDelta::seconds(earlier);
            let request = entries
                .into_iter()
                .fold(
                    UnpostedTransaction::builder(key("generated"), effective_at, "Generated"),
                    |builder, (account, side, cents, tags)| match side {
                        Side::Debit => builder.debit(account, usd(cents), tags),
                        Side::Credit => builder.credit(account, usd(cents), tags),
                    },
                )
                .build(&chart(), effective_at)
                .unwrap();
            let posted = request
                .clone()
                .post(
                    &chart(),
                    TransactionId::new(id),
                    effective_at + TimeDelta::seconds(later),
                )
                .unwrap();
            prop_assert!(same_contents(&request, &posted));
        }
    }
}
