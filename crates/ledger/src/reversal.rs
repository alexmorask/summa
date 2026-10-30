//! Reversals: correcting a posted transaction by posting its mirror image.
//!
//! A reversal cancels a transaction by mirroring it on every axis anyone
//! reports on: the same accounts and the same tags, with the sides flipped.
//! Mirroring only the accounts would zero the account balances but leave the
//! tagged ones (per customer, say) uncorrected. That a transaction is
//! reversed at most once is the database's job (M2), not this module's.

use std::collections::HashSet;
use std::fmt;

use chrono::{DateTime, Utc};

use crate::account::Side;
use crate::chart::Chart;
use crate::idempotency::IdempotencyKey;
use crate::money::Currency;
use crate::tag::Tag;
use crate::transaction::{
    Entry, PostedTransaction, TransactionError, TransactionId, UnpostedTransaction, utc,
};

impl<C: Currency, T: Tag> PostedTransaction<C, T> {
    /// Builds the transaction that reverses this one, named `key`: the same
    /// entries in the same order, with the sides flipped, linked back to this
    /// transaction.
    /// `build`'s checks run against `chart` and `now`, then the result is
    /// checked with [`UnpostedTransaction::check_reversal_of`], which rejects
    /// an `effective_at` before this transaction's.
    pub fn reversal(
        &self,
        key: IdempotencyKey,
        effective_at: DateTime<Utc>,
        description: impl Into<String>,
        chart: &Chart,
        now: DateTime<Utc>,
    ) -> Result<UnpostedTransaction<C, T>, ReversalError<C>> {
        let start =
            UnpostedTransaction::builder(key, effective_at, description).reversing(self.id());
        let reversal = self
            .entries()
            .iter()
            .fold(start, |builder, entry| {
                let tags = entry.tags().iter().cloned();
                match entry.side() {
                    Side::Debit => builder.credit(entry.account(), entry.amount(), tags),
                    Side::Credit => builder.debit(entry.account(), entry.amount(), tags),
                }
            })
            .build(chart, now)
            .map_err(ReversalError::Transaction)?;
        reversal.check_reversal_of(self)?;
        Ok(reversal)
    }
}

impl<C: Currency, T: Tag> UnpostedTransaction<C, T> {
    /// Checks that this transaction is a reversal of `original`. In order:
    ///
    /// 1. It says which transaction it reverses, and that is `original`.
    /// 2. Its effective time isn't before the original's. Equal is fine.
    /// 3. Its entries mirror the original's one for one: each original entry
    ///    has a partner with the same account and amount, the opposite side
    ///    and the same tags (as a set, so order doesn't matter), and no
    ///    entry is left over. Entry order is not compared.
    ///
    /// Each original entry, in order, takes the first unpaired partner. That
    /// is safe because mirroring is exact equality, so no other pairing could
    /// do better.
    pub fn check_reversal_of(
        &self,
        original: &PostedTransaction<C, T>,
    ) -> Result<(), ReversalError<C>> {
        let links_to = self.reverses().ok_or(ReversalError::NotAReversal)?;
        if links_to != original.id() {
            return Err(ReversalError::WrongOriginal {
                links_to,
                original: original.id(),
            });
        }
        if self.effective_at() < original.effective_at() {
            return Err(ReversalError::EffectiveBeforeOriginal {
                effective_at: self.effective_at(),
                original_effective_at: original.effective_at(),
            });
        }
        let mut paired = vec![false; self.entries().len()];
        for (index, entry) in original.entries().iter().enumerate() {
            let partner = self
                .entries()
                .iter()
                .enumerate()
                .find(|&(i, candidate)| !paired[i] && mirrors(entry, candidate));
            match partner {
                Some((i, _)) => paired[i] = true,
                None => return Err(ReversalError::UnmatchedOriginalEntry { entry: index }),
            }
        }
        match paired.iter().position(|&done| !done) {
            Some(entry) => Err(ReversalError::UnmatchedReversalEntry { entry }),
            None => Ok(()),
        }
    }
}

/// Whether `reversal` is the exact mirror of `original`. An entry can't hold
/// two tags of one kind, so a set of tags is a faithful way to compare them.
fn mirrors<C: Currency, T: Tag>(original: &Entry<C, T>, reversal: &Entry<C, T>) -> bool {
    original.account() == reversal.account()
        && original.amount() == reversal.amount()
        && original.side() == reversal.side().opposite()
        && original.tags().iter().collect::<HashSet<_>>()
            == reversal.tags().iter().collect::<HashSet<_>>()
}

/// Why a transaction isn't a valid reversal. `entry` is the entry's 0-based
/// position; messages show it 1-based.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReversalError<C: Currency> {
    /// The transaction doesn't link to any transaction.
    NotAReversal,
    /// The transaction links to a different transaction than the one checked.
    WrongOriginal {
        links_to: TransactionId,
        original: TransactionId,
    },
    /// The reversal is effective before the original.
    EffectiveBeforeOriginal {
        effective_at: DateTime<Utc>,
        original_effective_at: DateTime<Utc>,
    },
    /// An entry of the original has no mirror among the reversal's entries.
    UnmatchedOriginalEntry { entry: usize },
    /// An entry of the reversal mirrors no entry of the original.
    UnmatchedReversalEntry { entry: usize },
    /// The reversal failed `build`.
    Transaction(TransactionError<C>),
}

impl<C: Currency> fmt::Display for ReversalError<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReversalError::NotAReversal => {
                write!(
                    f,
                    "the transaction doesn't say which transaction it reverses"
                )
            }
            ReversalError::WrongOriginal { links_to, original } => write!(
                f,
                "reverses transaction {}, not transaction {}",
                links_to.get(),
                original.get()
            ),
            ReversalError::EffectiveBeforeOriginal {
                effective_at,
                original_effective_at,
            } => write!(
                f,
                "effective time {} is before the original's ({})",
                utc(*effective_at),
                utc(*original_effective_at)
            ),
            ReversalError::UnmatchedOriginalEntry { entry } => {
                write!(
                    f,
                    "original entry {} has no mirror in the reversal",
                    entry + 1
                )
            }
            ReversalError::UnmatchedReversalEntry { entry } => {
                write!(
                    f,
                    "reversal entry {} mirrors no entry in the original",
                    entry + 1
                )
            }
            ReversalError::Transaction(error) => error.fmt(f),
        }
    }
}

impl<C: Currency> std::error::Error for ReversalError<C> {}

#[cfg(test)]
mod tests {
    use crate::account::{AccountNumber, Side};
    use crate::money::Usd;
    use crate::test_support::*;
    use crate::transaction::{
        PostedTransaction, TransactionError, TransactionId, UnpostedTransaction,
    };
    use crate::{Account, AccountType, Chart, ReversalError};
    use chrono::{DateTime, TimeDelta, Utc};
    use proptest::prelude::*;

    type Posted = PostedTransaction<Usd, TestTag>;
    type Unposted = UnpostedTransaction<Usd, TestTag>;

    fn id(n: u64) -> TransactionId {
        TransactionId::new(n)
    }

    /// Debit asset 2,160; credit liability 2,000 and revenue 160.
    fn original_entries() -> Vec<TestEntry> {
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

    fn hand_built(
        entries: &[TestEntry],
        link: Option<u64>,
        effective_at: DateTime<Utc>,
    ) -> Result<Unposted, TransactionError<Usd>> {
        let mut builder = UnpostedTransaction::builder(key("hand-built"), effective_at, "Reversal");
        if let Some(link) = link {
            builder = builder.reversing(id(link));
        }
        entries
            .iter()
            .cloned()
            .fold(builder, |b, (account, side, cents, tags)| match side {
                Side::Debit => b.debit(account, usd(cents), tags),
                Side::Credit => b.credit(account, usd(cents), tags),
            })
            .build(&chart(), noon())
    }

    fn posted(entries: &[TestEntry], id: u64) -> Posted {
        let unposted = hand_built(entries, None, noon()).unwrap();
        unposted
            .post(&chart(), TransactionId::new(id), noon())
            .unwrap()
    }

    fn posted_original() -> Posted {
        posted(&original_entries(), 1)
    }

    fn flipped(entries: &[TestEntry]) -> Vec<TestEntry> {
        entries
            .iter()
            .map(|(account, side, cents, tags)| (*account, side.opposite(), *cents, tags.clone()))
            .collect()
    }

    fn day() -> TimeDelta {
        TimeDelta::days(1)
    }

    // --- Building ---

    #[test]
    fn builds_the_mirror_image() {
        let original = posted_original();
        let reversal = original
            .reversal(
                key("reversal"),
                noon() + day(),
                "Reverses 1",
                &chart(),
                noon() + day(),
            )
            .unwrap();
        let sides: Vec<Side> = reversal.entries().iter().map(|e| e.side()).collect();
        assert_eq!(sides, [Side::Credit, Side::Debit, Side::Debit]);
        for (mirror, entry) in reversal.entries().iter().zip(original.entries()) {
            assert_eq!(mirror.account(), entry.account());
            assert_eq!(mirror.amount(), entry.amount());
            assert_eq!(mirror.tags(), entry.tags());
        }
        assert_eq!(reversal.reverses(), Some(id(1)));
        assert_eq!(reversal.description(), "Reverses 1");
        assert_eq!(reversal.effective_at(), noon() + day());
    }

    #[test]
    fn a_reversal_may_share_the_originals_effective_time() {
        let result =
            posted_original().reversal(key("reversal"), noon(), "Reverses 1", &chart(), noon());
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_a_reversal_effective_before_the_original() {
        let early = noon() - TimeDelta::seconds(1);
        let result =
            posted_original().reversal(key("reversal"), early, "Reverses 1", &chart(), noon());
        assert_eq!(
            result,
            Err(ReversalError::EffectiveBeforeOriginal {
                effective_at: early,
                original_effective_at: noon(),
            })
        );
    }

    #[test]
    fn rejects_a_reversal_effective_in_the_future() {
        let now = noon() + TimeDelta::hours(1);
        let result = posted_original().reversal(
            key("reversal"),
            noon() + TimeDelta::hours(2),
            "Reverses 1",
            &chart(),
            now,
        );
        assert!(matches!(
            result,
            Err(ReversalError::Transaction(
                TransactionError::EffectiveTimeInFuture { .. }
            ))
        ));
    }

    #[test]
    fn rejects_a_reversal_against_a_chart_missing_an_account() {
        let without_revenue = Chart::new([
            Account::new(ASSET, "Asset", AccountType::Asset),
            Account::new(LIABILITY, "Liability", AccountType::Liability),
        ])
        .unwrap();
        let result = posted_original().reversal(
            key("reversal"),
            noon(),
            "Reverses 1",
            &without_revenue,
            noon(),
        );
        assert_eq!(
            result,
            Err(ReversalError::Transaction(
                TransactionError::UnknownAccount {
                    entry: 2,
                    account: REVENUE,
                }
            ))
        );
    }

    #[test]
    fn a_reversal_can_itself_be_reversed() {
        let original = posted_original();
        let reversal = original
            .reversal(key("reversal"), noon(), "Reverses 1", &chart(), noon())
            .unwrap();
        let posted_reversal = reversal.post(&chart(), id(2), noon()).unwrap();
        let again = posted_reversal
            .reversal(key("reversal-2"), noon(), "Reverses 2", &chart(), noon())
            .unwrap();
        assert_eq!(again.reverses(), Some(id(2)));
        assert_eq!(again.entries(), original.entries());
    }

    // --- Checking ---

    fn check(entries: &[TestEntry], link: Option<u64>) -> Result<(), ReversalError<Usd>> {
        hand_built(entries, link, noon())
            .unwrap()
            .check_reversal_of(&posted_original())
    }

    #[test]
    fn check_accepts_entries_and_tags_in_any_order() {
        let entries = vec![
            (REVENUE, Side::Debit, 160, vec![TestTag::Alpha(42)]),
            (ASSET, Side::Credit, 2_160, vec![TestTag::Alpha(42)]),
            (
                LIABILITY,
                Side::Debit,
                2_000,
                vec![TestTag::Beta(9), TestTag::Alpha(42)],
            ),
        ];
        assert_eq!(check(&entries, Some(1)), Ok(()));
    }

    #[test]
    fn check_rejects_a_transaction_with_no_link() {
        assert_eq!(
            check(&flipped(&original_entries()), None),
            Err(ReversalError::NotAReversal)
        );
    }

    #[test]
    fn check_rejects_a_link_to_another_transaction() {
        assert_eq!(
            check(&flipped(&original_entries()), Some(2)),
            Err(ReversalError::WrongOriginal {
                links_to: id(2),
                original: id(1),
            })
        );
    }

    #[test]
    fn check_rejects_a_changed_tag() {
        let mut entries = flipped(&original_entries());
        entries[1].3 = vec![TestTag::Alpha(43), TestTag::Beta(9)];
        assert_eq!(
            check(&entries, Some(1)),
            Err(ReversalError::UnmatchedOriginalEntry { entry: 1 })
        );
    }

    #[test]
    fn check_rejects_a_changed_account() {
        let mut entries = flipped(&original_entries());
        entries[2].0 = CONTRA_REVENUE;
        assert_eq!(
            check(&entries, Some(1)),
            Err(ReversalError::UnmatchedOriginalEntry { entry: 2 })
        );
    }

    #[test]
    fn a_changed_single_amount_is_rejected_as_unbalanced() {
        let mut entries = flipped(&original_entries());
        entries[1].2 = 2_161;
        assert!(matches!(
            hand_built(&entries, Some(1), noon()),
            Err(TransactionError::Unbalanced { .. })
        ));
    }

    #[test]
    fn check_rejects_an_extra_entry() {
        let mut entries = flipped(&original_entries());
        entries.push((ASSET, Side::Debit, 5, vec![]));
        entries.push((LIABILITY, Side::Credit, 5, vec![]));
        assert_eq!(
            check(&entries, Some(1)),
            Err(ReversalError::UnmatchedReversalEntry { entry: 3 })
        );
    }

    #[test]
    fn check_pairs_each_entry_once() {
        let original = posted(
            &[
                (ASSET, Side::Debit, 500, vec![]),
                (ASSET, Side::Debit, 500, vec![]),
                (REVENUE, Side::Credit, 1_000, vec![]),
            ],
            1,
        );
        let reversal = hand_built(
            &[
                (ASSET, Side::Credit, 500, vec![]),
                (LIABILITY, Side::Credit, 500, vec![]),
                (REVENUE, Side::Debit, 1_000, vec![]),
            ],
            Some(1),
            noon(),
        )
        .unwrap();
        assert_eq!(
            reversal.check_reversal_of(&original),
            Err(ReversalError::UnmatchedOriginalEntry { entry: 1 })
        );
    }

    // --- Messages ---

    #[test]
    fn reversal_errors_explain_themselves() {
        let cases: [(ReversalError<Usd>, &str); 6] = [
            (
                ReversalError::NotAReversal,
                "the transaction doesn't say which transaction it reverses",
            ),
            (
                ReversalError::WrongOriginal {
                    links_to: id(2),
                    original: id(1),
                },
                "reverses transaction 2, not transaction 1",
            ),
            (
                ReversalError::EffectiveBeforeOriginal {
                    effective_at: noon() - day(),
                    original_effective_at: noon(),
                },
                "effective time 2026-10-15T12:00:00Z is before the original's (2026-10-16T12:00:00Z)",
            ),
            (
                ReversalError::UnmatchedOriginalEntry { entry: 1 },
                "original entry 2 has no mirror in the reversal",
            ),
            (
                ReversalError::UnmatchedReversalEntry { entry: 3 },
                "reversal entry 4 mirrors no entry in the original",
            ),
            (
                ReversalError::Transaction(TransactionError::TooFewEntries { count: 1 }),
                "a transaction needs at least 2 entries, got 1",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    // --- Rules that must always hold ---

    /// One thing wrong with a correct reversal. `Index` picks which entry.
    #[derive(Clone, Debug)]
    enum Breakage {
        Account(prop::sample::Index),
        AddTag(prop::sample::Index),
        RemoveTag(prop::sample::Index),
        ChangeTagValue(prop::sample::Index),
        ShiftAmount(prop::sample::Index, prop::sample::Index, i64),
        ExtraPair(i64),
        WrongLink,
        NoLink,
        EarlyEffective(i64),
    }

    fn any_breakage() -> impl Strategy<Value = Breakage> {
        let index = || any::<prop::sample::Index>();
        prop_oneof![
            index().prop_map(Breakage::Account),
            index().prop_map(Breakage::AddTag),
            index().prop_map(Breakage::RemoveTag),
            index().prop_map(Breakage::ChangeTagValue),
            (index(), index(), 1_i64..=1_000).prop_map(|(d, c, n)| Breakage::ShiftAmount(d, c, n)),
            (1_i64..=1_000).prop_map(Breakage::ExtraPair),
            Just(Breakage::WrongLink),
            Just(Breakage::NoLink),
            (1_i64..=86_400).prop_map(Breakage::EarlyEffective),
        ]
    }

    /// The tag with the same kind and a different value.
    fn changed(tag: &TestTag) -> TestTag {
        match tag {
            TestTag::Alpha(n) => TestTag::Alpha(n + 1),
            TestTag::Beta(n) => TestTag::Beta(n + 1),
        }
    }

    /// The indexes of the entries on one side.
    fn on_side(entries: &[TestEntry], side: Side) -> Vec<usize> {
        (0..entries.len())
            .filter(|&i| entries[i].1 == side)
            .collect()
    }

    proptest! {
        /// A reversal `reversal()` builds always passes the check.
        #[test]
        fn built_reversals_always_pass_the_check(entries in valid_entries()) {
            let original = posted(&entries, 1);
            let reversal = original
                .reversal(key("reversal"), noon(), "Reverses 1", &chart(), noon())
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(reversal.check_reversal_of(&original), Ok(()));
        }

        /// Starting from a correct reversal and breaking one thing, the check
        /// names what was broken. Every breakage keeps the reversal buildable
        /// and balanced, so the check, not `build`, has to catch it.
        #[test]
        fn a_broken_reversal_is_rejected_for_what_was_broken(
            entries in valid_entries(),
            breakage in any_breakage(),
        ) {
            let original = posted(&entries, 1);
            let mut reversed = flipped(&entries);
            let mut link = Some(1);
            let mut effective_at = noon();
            let mut expected_mirror_error = true;
            match &breakage {
                Breakage::Account(index) => {
                    let entry = index.index(reversed.len());
                    // The next chart account, wrapping 4 to 1.
                    reversed[entry].0 = AccountNumber::new(reversed[entry].0.get() % 4 + 1);
                }
                Breakage::AddTag(index) => {
                    let entry = index.index(reversed.len());
                    let tags = &mut reversed[entry].3;
                    let has = |tags: &[TestTag], alpha: bool| {
                        tags.iter().any(|t| matches!(t, TestTag::Alpha(_)) == alpha)
                    };
                    prop_assume!(!(has(tags, true) && has(tags, false)));
                    tags.push(if has(tags, true) {
                        TestTag::Beta(7)
                    } else {
                        TestTag::Alpha(7)
                    });
                }
                Breakage::RemoveTag(index) => {
                    let entry = index.index(reversed.len());
                    prop_assume!(!reversed[entry].3.is_empty());
                    reversed[entry].3.pop();
                }
                Breakage::ChangeTagValue(index) => {
                    let entry = index.index(reversed.len());
                    prop_assume!(!reversed[entry].3.is_empty());
                    let tag = changed(&reversed[entry].3[0]);
                    reversed[entry].3[0] = tag;
                }
                Breakage::ShiftAmount(debit, credit, cents) => {
                    let debits = on_side(&reversed, Side::Debit);
                    let credits = on_side(&reversed, Side::Credit);
                    reversed[debits[debit.index(debits.len())]].2 += cents;
                    reversed[credits[credit.index(credits.len())]].2 += cents;
                }
                Breakage::ExtraPair(cents) => {
                    reversed.push((ASSET, Side::Debit, *cents, vec![]));
                    reversed.push((LIABILITY, Side::Credit, *cents, vec![]));
                }
                Breakage::WrongLink => {
                    link = Some(2);
                    expected_mirror_error = false;
                }
                Breakage::NoLink => {
                    link = None;
                    expected_mirror_error = false;
                }
                Breakage::EarlyEffective(seconds) => {
                    effective_at = noon() - TimeDelta::seconds(*seconds);
                    expected_mirror_error = false;
                }
            }
            let reversal = hand_built(&reversed, link, effective_at)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            let result = reversal.check_reversal_of(&original);
            if expected_mirror_error {
                let extra_only = matches!(breakage, Breakage::ExtraPair(_));
                let ok = match &result {
                    Err(ReversalError::UnmatchedReversalEntry { .. }) => true,
                    Err(ReversalError::UnmatchedOriginalEntry { .. }) => !extra_only,
                    _ => false,
                };
                prop_assert!(ok, "unexpected result {:?}", result);
            } else {
                let expected = match breakage {
                    Breakage::WrongLink => ReversalError::WrongOriginal {
                        links_to: id(2),
                        original: id(1),
                    },
                    Breakage::NoLink => ReversalError::NotAReversal,
                    _ => ReversalError::EffectiveBeforeOriginal {
                        effective_at,
                        original_effective_at: noon(),
                    },
                };
                prop_assert_eq!(result, Err(expected));
            }
        }
    }
}
