//! Transactions: two or more entries that balance. A transaction is built and
//! checked as an [`UnpostedTransaction`]; only the ledger posts it.

use std::collections::HashSet;
use std::fmt;

use chrono::{DateTime, SecondsFormat, Utc};

use crate::account::{AccountNumber, Side};
use crate::chart::Chart;
use crate::money::{Currency, Money, MoneyError};
use crate::tag::Tag;

/// The identity of a posted transaction. The ledger stamps it at posting, like
/// the recorded time, and pure code never makes one up (ADR 0003, ADR 0006).
/// It is opaque: the only things to do with one are compare it and pass it
/// around.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransactionId(u64);

impl TransactionId {
    /// Wraps a number the ledger assigned.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// The number inside, for storage and display.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One line of a transaction: an account, a side, a positive amount, and tags
/// saying who or what it's about.
///
/// The account is referenced by number. The [`Chart`] holds the account
/// itself, so there's only one copy of it (ADR 0005).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry<C: Currency, T: Tag> {
    account: AccountNumber,
    side: Side,
    amount: Money<C>,
    tags: Vec<T>,
}

impl<C: Currency, T: Tag> Entry<C, T> {
    pub const fn account(&self) -> AccountNumber {
        self.account
    }

    pub const fn side(&self) -> Side {
        self.side
    }

    pub const fn amount(&self) -> Money<C> {
        self.amount
    }

    pub const fn tags(&self) -> &[T] {
        self.tags.as_slice()
    }
}

/// A transaction that passed every `build` check but hasn't been posted: it has an
/// effective time (when it happened) but no recorded time yet. The only way to
/// get one is [`TransactionBuilder::build`].
///
/// "Unposted" is the accounting term. Unlike a draft journal in accounting
/// software, it can't be edited and always balances.
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use ledger::{Account, AccountNumber, AccountType, Chart, Money, Tag, UnpostedTransaction, Usd};
///
/// #[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// enum ExampleTag {
///     Customer(u64),
/// }
///
/// impl Tag for ExampleTag {
///     fn kind(&self) -> &'static str {
///         match self {
///             ExampleTag::Customer(_) => "customer",
///         }
///     }
/// }
///
/// let receivable = AccountNumber::new(1100);
/// let tax = AccountNumber::new(2100);
/// let revenue = AccountNumber::new(4000);
/// let chart = Chart::new([
///     Account::new(receivable, "Receivable", AccountType::Asset),
///     Account::new(tax, "Tax Payable", AccountType::Liability),
///     Account::new(revenue, "Revenue", AccountType::Revenue),
/// ])
/// .unwrap();
/// let now = Utc.with_ymd_and_hms(2026, 9, 30, 12, 0, 0).unwrap();
///
/// // $21.60 = $20.00 + $1.60
/// let sale = UnpostedTransaction::builder(now, "A taxed sale")
///     .debit(receivable, Money::<Usd>::from_minor_units(2_160), [ExampleTag::Customer(42)])
///     .credit(revenue, Money::from_minor_units(2_000), [ExampleTag::Customer(42)])
///     .credit(tax, Money::from_minor_units(160), [ExampleTag::Customer(42)])
///     .build(&chart, now)
///     .unwrap();
/// assert_eq!(sale.entries().len(), 3);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnpostedTransaction<C: Currency, T: Tag> {
    effective_at: DateTime<Utc>,
    description: String,
    entries: Vec<Entry<C, T>>,
    reverses: Option<TransactionId>,
}

impl<C: Currency, T: Tag> UnpostedTransaction<C, T> {
    /// Starts a transaction that happened at `effective_at`, with a
    /// description for humans. Add entries with `.debit(...)` and
    /// `.credit(...)`, then check everything with `.build(...)`.
    pub fn builder(
        effective_at: DateTime<Utc>,
        description: impl Into<String>,
    ) -> TransactionBuilder<C, T> {
        TransactionBuilder {
            effective_at,
            description: description.into(),
            entries: Vec::new(),
            reverses: None,
        }
    }

    /// When the event happened in the business.
    pub const fn effective_at(&self) -> DateTime<Utc> {
        self.effective_at
    }

    /// Free text for humans. Never used by the rules.
    pub const fn description(&self) -> &str {
        self.description.as_str()
    }

    /// The entries, in the order they were added.
    pub const fn entries(&self) -> &[Entry<C, T>] {
        self.entries.as_slice()
    }

    /// The transaction this one reverses, if it is a reversal. The link lives
    /// on the reversal because the original can never change (ADR 0006).
    pub const fn reverses(&self) -> Option<TransactionId> {
        self.reverses
    }

    /// Posts the transaction into a ledger with this `chart`: stamps the ID
    /// the ledger gave it and the time the ledger recorded it.
    ///
    /// The rules about the transaction itself (balance, positive amounts, two
    /// or more entries, no repeated tag kind) held at `build` and can't stop
    /// holding, because the transaction can't change. The rules about its
    /// surroundings are checked again here, against the ledger's own clock
    /// and chart; `build`'s checks against the caller's `now` and chart are
    /// early feedback. In order:
    ///
    /// 1. Nothing may be recorded as having happened after it was written
    ///    down, so `effective_at` must not be after `recorded_at`.
    /// 2. Every entry's account must be in the ledger's chart, which may not
    ///    be the chart the transaction was built against.
    ///
    /// Private to the ledger crate, so no other crate can post or choose an ID
    /// or a recorded time. Ledger storage calls it.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "called by ledger storage from M2 (SUM-9)")
    )]
    pub(crate) fn post(
        self,
        chart: &Chart,
        id: TransactionId,
        recorded_at: DateTime<Utc>,
    ) -> Result<PostedTransaction<C, T>, PostError> {
        if self.effective_at > recorded_at {
            return Err(PostError::EffectiveAfterRecorded {
                effective_at: self.effective_at,
                recorded_at,
            });
        }
        for (index, entry) in self.entries.iter().enumerate() {
            if chart.get(entry.account).is_none() {
                return Err(PostError::UnknownAccount {
                    entry: index,
                    account: entry.account,
                });
            }
        }
        Ok(PostedTransaction {
            id,
            transaction: self,
            recorded_at,
        })
    }
}

/// A transaction the ledger has posted: everything the unposted transaction
/// had, plus its ID and the time it was recorded. Only the ledger can create one.
///
/// From outside the ledger crate, a transaction can be built:
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use ledger::{Account, AccountNumber, AccountType, Chart, Money, Tag, UnpostedTransaction, Usd};
///
/// #[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// enum ExampleTag {
///     Customer(u64),
/// }
///
/// impl Tag for ExampleTag {
///     fn kind(&self) -> &'static str {
///         match self {
///             ExampleTag::Customer(_) => "customer",
///         }
///     }
/// }
///
/// let cash = AccountNumber::new(1);
/// let sales = AccountNumber::new(2);
/// let chart = Chart::new([
///     Account::new(cash, "Cash", AccountType::Asset),
///     Account::new(sales, "Sales", AccountType::Revenue),
/// ])
/// .unwrap();
/// let now = Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();
/// let amount = Money::<Usd>::from_minor_units(1_000);
/// let unposted = UnpostedTransaction::builder(now, "A sale")
///     .debit(cash, amount, [ExampleTag::Customer(42)])
///     .credit(sales, amount, [ExampleTag::Customer(42)])
///     .build(&chart, now)
///     .unwrap();
/// ```
///
/// but the same code can't post it:
///
/// ```compile_fail
/// use chrono::{TimeZone, Utc};
/// use ledger::{Account, AccountNumber, AccountType, Chart, Money, Tag, TransactionId, UnpostedTransaction, Usd};
///
/// #[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// enum ExampleTag {
///     Customer(u64),
/// }
///
/// impl Tag for ExampleTag {
///     fn kind(&self) -> &'static str {
///         match self {
///             ExampleTag::Customer(_) => "customer",
///         }
///     }
/// }
///
/// let cash = AccountNumber::new(1);
/// let sales = AccountNumber::new(2);
/// let chart = Chart::new([
///     Account::new(cash, "Cash", AccountType::Asset),
///     Account::new(sales, "Sales", AccountType::Revenue),
/// ])
/// .unwrap();
/// let now = Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();
/// let amount = Money::<Usd>::from_minor_units(1_000);
/// let unposted = UnpostedTransaction::builder(now, "A sale")
///     .debit(cash, amount, [ExampleTag::Customer(42)])
///     .credit(sales, amount, [ExampleTag::Customer(42)])
///     .build(&chart, now)
///     .unwrap();
/// let _posted = unposted.post(&chart, TransactionId::new(1), now);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostedTransaction<C: Currency, T: Tag> {
    id: TransactionId,
    transaction: UnpostedTransaction<C, T>,
    recorded_at: DateTime<Utc>,
}

impl<C: Currency, T: Tag> PostedTransaction<C, T> {
    /// The ID the ledger gave it when it was posted.
    pub const fn id(&self) -> TransactionId {
        self.id
    }

    /// The transaction this one reverses, if it is a reversal.
    pub const fn reverses(&self) -> Option<TransactionId> {
        self.transaction.reverses
    }

    /// When the event happened in the business.
    pub const fn effective_at(&self) -> DateTime<Utc> {
        self.transaction.effective_at
    }

    /// When the ledger recorded it. Set by the ledger, never by the caller.
    pub const fn recorded_at(&self) -> DateTime<Utc> {
        self.recorded_at
    }

    /// Free text for humans. Never used by the rules.
    pub const fn description(&self) -> &str {
        self.transaction.description()
    }

    /// The entries, in the order they were added.
    pub const fn entries(&self) -> &[Entry<C, T>] {
        self.transaction.entries()
    }
}

/// Collects a transaction's parts. Nothing is checked until
/// [`build`](Self::build).
#[derive(Clone, Debug)]
#[must_use = "a transaction builder does nothing until `.build()` is called"]
pub struct TransactionBuilder<C: Currency, T: Tag> {
    effective_at: DateTime<Utc>,
    description: String,
    entries: Vec<Entry<C, T>>,
    reverses: Option<TransactionId>,
}

impl<C: Currency, T: Tag> TransactionBuilder<C, T> {
    /// Marks the transaction as reversing `original`. Nothing is checked at
    /// `build`: the builder can't see the original.
    pub fn reversing(mut self, original: TransactionId) -> Self {
        self.reverses = Some(original);
        self
    }

    /// Adds a debit entry.
    pub fn debit(
        self,
        account: AccountNumber,
        amount: Money<C>,
        tags: impl IntoIterator<Item = T>,
    ) -> Self {
        self.push(account, Side::Debit, amount, tags)
    }

    /// Adds a credit entry.
    pub fn credit(
        self,
        account: AccountNumber,
        amount: Money<C>,
        tags: impl IntoIterator<Item = T>,
    ) -> Self {
        self.push(account, Side::Credit, amount, tags)
    }

    fn push(
        mut self,
        account: AccountNumber,
        side: Side,
        amount: Money<C>,
        tags: impl IntoIterator<Item = T>,
    ) -> Self {
        self.entries.push(Entry {
            account,
            side,
            amount,
            tags: tags.into_iter().collect(),
        });
        self
    }

    /// Checks every rule against `chart` (the accounts entries may use) and
    /// `now` (the caller's current time), and returns the transaction or the
    /// first problem found. The checks run in this order:
    ///
    /// 1. At least two entries.
    /// 2. The effective time isn't after `now`.
    /// 3. Each entry, in the order added: its account is in `chart`, then its
    ///    amount is positive, then no tag kind appears twice on it.
    /// 4. The totals: debits, then credits, each fit in an `i64`; then debits
    ///    equal credits.
    ///
    /// An entry problem often explains a bad total, and totals only mean
    /// something once every amount is positive.
    pub fn build(
        self,
        chart: &Chart,
        now: DateTime<Utc>,
    ) -> Result<UnpostedTransaction<C, T>, TransactionError<C>> {
        let count = self.entries.len();
        if count < 2 {
            return Err(TransactionError::TooFewEntries { count });
        }
        if self.effective_at > now {
            return Err(TransactionError::EffectiveTimeInFuture {
                effective_at: self.effective_at,
                now,
            });
        }
        for (index, entry) in self.entries.iter().enumerate() {
            if chart.get(entry.account).is_none() {
                return Err(TransactionError::UnknownAccount {
                    entry: index,
                    account: entry.account,
                });
            }
            if entry.amount.minor_units() <= 0 {
                return Err(TransactionError::NonPositiveAmount {
                    entry: index,
                    amount: entry.amount,
                });
            }
            if let Some(kind) = repeated_kind(&entry.tags) {
                return Err(TransactionError::DuplicateTagKind { entry: index, kind });
            }
        }
        let debits = total(&self.entries, Side::Debit)?;
        let credits = total(&self.entries, Side::Credit)?;
        if debits != credits {
            return Err(TransactionError::Unbalanced { debits, credits });
        }
        Ok(UnpostedTransaction {
            effective_at: self.effective_at,
            description: self.description,
            entries: self.entries,
            reverses: self.reverses,
        })
    }
}

/// The first tag kind that appears more than once, if any.
fn repeated_kind<T: Tag>(tags: &[T]) -> Option<&'static str> {
    let mut seen = HashSet::new();
    tags.iter().map(Tag::kind).find(|&kind| !seen.insert(kind))
}

/// The sum of the amounts on one side, or `Overflow` naming that side if it
/// doesn't fit.
fn total<C: Currency, T: Tag>(
    entries: &[Entry<C, T>],
    side: Side,
) -> Result<Money<C>, TransactionError<C>> {
    entries
        .iter()
        .filter(|entry| entry.side == side)
        .try_fold(Money::from_minor_units(0), |sum, entry| {
            sum.checked_add(entry.amount)
        })
        // `MoneyError` has one variant, so this pattern is exhaustive; a new
        // variant would make it a compile error here.
        .map_err(|MoneyError::Overflow| TransactionError::Overflow { side })
}

/// Why `build` rejected a transaction. `entry` is the entry's 0-based
/// position; messages show it 1-based.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionError<C: Currency> {
    /// Fewer than two entries.
    TooFewEntries { count: usize },
    /// The effective time is after the `now` passed to `build`.
    EffectiveTimeInFuture {
        effective_at: DateTime<Utc>,
        now: DateTime<Utc>,
    },
    /// An entry's account isn't in the chart.
    UnknownAccount {
        entry: usize,
        account: AccountNumber,
    },
    /// An entry's amount is zero or negative. Direction comes from the side.
    NonPositiveAmount { entry: usize, amount: Money<C> },
    /// An entry has two tags of the same kind, which would count its amount
    /// twice in that kind's balances.
    DuplicateTagKind { entry: usize, kind: &'static str },
    /// The total on `side` doesn't fit in an `i64` of minor units. Debits are
    /// totalled first.
    Overflow { side: Side },
    /// Total debits don't equal total credits.
    Unbalanced { debits: Money<C>, credits: Money<C> },
}

impl<C: Currency> fmt::Display for TransactionError<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionError::TooFewEntries { count } => {
                write!(f, "a transaction needs at least 2 entries, got {count}")
            }
            TransactionError::EffectiveTimeInFuture { effective_at, now } => write!(
                f,
                "effective time {} is after now ({})",
                utc(*effective_at),
                utc(*now)
            ),
            TransactionError::UnknownAccount { entry, account } => write!(
                f,
                "entry {}: account {} is not in the chart",
                entry + 1,
                account.get()
            ),
            TransactionError::NonPositiveAmount { entry, amount } => write!(
                f,
                "entry {}: amount must be positive, got {amount}",
                entry + 1
            ),
            TransactionError::DuplicateTagKind { entry, kind } => {
                write!(f, "entry {}: more than one {kind} tag", entry + 1)
            }
            TransactionError::Overflow { side } => {
                let side = match side {
                    Side::Debit => "debits",
                    Side::Credit => "credits",
                };
                write!(f, "total {side} are too large for the ledger to hold")
            }
            TransactionError::Unbalanced { debits, credits } => {
                write!(f, "debits {debits} ≠ credits {credits}")
            }
        }
    }
}

impl<C: Currency> std::error::Error for TransactionError<C> {}

/// Why `post` rejected a transaction. Separate from [`TransactionError`] so
/// each step's error type lists only what that step can return. `entry` is the
/// entry's 0-based position; messages show it 1-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostError {
    /// The recorded time is before the effective time.
    EffectiveAfterRecorded {
        effective_at: DateTime<Utc>,
        recorded_at: DateTime<Utc>,
    },
    /// An entry's account isn't in the ledger's chart.
    UnknownAccount {
        entry: usize,
        account: AccountNumber,
    },
}

impl fmt::Display for PostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PostError::EffectiveAfterRecorded {
                effective_at,
                recorded_at,
            } => write!(
                f,
                "effective time {} is after recorded time ({})",
                utc(*effective_at),
                utc(*recorded_at)
            ),
            PostError::UnknownAccount { entry, account } => write!(
                f,
                "entry {}: account {} is not in the ledger's chart",
                entry + 1,
                account.get()
            ),
        }
    }
}

impl std::error::Error for PostError {}

/// RFC 3339 in UTC with a `Z`, showing fractional seconds only when present.
pub(crate) fn utc(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{Account, AccountType};
    use crate::money::Usd;
    use crate::test_support::*;
    use chrono::{TimeDelta, TimeZone};
    use proptest::prelude::*;

    fn builder() -> TransactionBuilder<Usd, TestTag> {
        UnpostedTransaction::builder(noon(), "Test")
    }

    /// $10.00 debit to the asset, $10.00 credit to revenue.
    fn balanced() -> TransactionBuilder<Usd, TestTag> {
        builder()
            .debit(ASSET, usd(1_000), [])
            .credit(REVENUE, usd(1_000), [])
    }

    // --- Accepted ---

    #[test]
    fn builds_a_balanced_two_entry_transaction() {
        let t = balanced().build(&chart(), noon()).unwrap();
        assert_eq!(t.entries().len(), 2);
        assert_eq!(t.effective_at(), noon());
        assert_eq!(t.description(), "Test");
    }

    #[test]
    fn builds_a_three_entry_transaction() {
        // Debits 2,160 cents; credits 2,000 + 160 = 2,160 cents.
        let t = builder()
            .debit(ASSET, usd(2_160), [TestTag::Alpha(42)])
            .credit(
                LIABILITY,
                usd(2_000),
                [TestTag::Alpha(42), TestTag::Beta(9)],
            )
            .credit(REVENUE, usd(160), [TestTag::Alpha(42)])
            .build(&chart(), noon())
            .unwrap();
        assert_eq!(t.entries().len(), 3);
    }

    #[test]
    fn keeps_entries_in_order_with_their_tags() {
        let t = builder()
            .debit(ASSET, usd(1_000), [TestTag::Alpha(1)])
            .credit(REVENUE, usd(1_000), [TestTag::Beta(2)])
            .build(&chart(), noon())
            .unwrap();
        let [first, second] = t.entries() else {
            panic!("expected two entries");
        };
        assert_eq!(first.account(), ASSET);
        assert_eq!(first.side(), Side::Debit);
        assert_eq!(first.amount(), usd(1_000));
        assert_eq!(first.tags(), &[TestTag::Alpha(1)]);
        assert_eq!(second.account(), REVENUE);
        assert_eq!(second.side(), Side::Credit);
        assert_eq!(second.tags(), &[TestTag::Beta(2)]);
    }

    #[test]
    fn accepts_effective_time_equal_to_now() {
        assert!(balanced().build(&chart(), noon()).is_ok());
    }

    #[test]
    fn accepts_effective_time_before_now() {
        let later = noon() + TimeDelta::days(3);
        assert!(balanced().build(&chart(), later).is_ok());
    }

    #[test]
    fn accepts_tags_of_different_kinds_on_one_entry() {
        let result = builder()
            .debit(ASSET, usd(1_000), [TestTag::Alpha(1), TestTag::Beta(1)])
            .credit(REVENUE, usd(1_000), [])
            .build(&chart(), noon());
        assert!(result.is_ok());
    }

    #[test]
    fn accepts_entries_without_tags() {
        let t = balanced().build(&chart(), noon()).unwrap();
        assert!(t.entries().iter().all(|e| e.tags().is_empty()));
    }

    #[test]
    fn accepts_totals_exactly_at_the_limit() {
        let result = builder()
            .debit(ASSET, usd(i64::MAX - 1), [])
            .debit(ASSET, usd(1), [])
            .credit(REVENUE, usd(i64::MAX), [])
            .build(&chart(), noon());
        assert!(result.is_ok());
    }

    #[test]
    fn accepts_the_same_account_on_both_sides() {
        let result = builder()
            .debit(ASSET, usd(500), [])
            .credit(ASSET, usd(500), [])
            .build(&chart(), noon());
        assert!(result.is_ok());
    }

    #[test]
    fn accepts_an_empty_description() {
        let t = UnpostedTransaction::<Usd, TestTag>::builder(noon(), "")
            .debit(ASSET, usd(1), [])
            .credit(REVENUE, usd(1), [])
            .build(&chart(), noon())
            .unwrap();
        assert_eq!(t.description(), "");
    }

    // --- Rejected, one per rule ---

    #[test]
    fn rejects_no_entries() {
        let result = builder().build(&chart(), noon());
        assert_eq!(result, Err(TransactionError::TooFewEntries { count: 0 }));
    }

    #[test]
    fn rejects_one_entry() {
        let result = builder()
            .debit(ASSET, usd(1_000), [])
            .build(&chart(), noon());
        assert_eq!(result, Err(TransactionError::TooFewEntries { count: 1 }));
    }

    #[test]
    fn rejects_effective_time_after_now() {
        let earlier = noon() - TimeDelta::seconds(1);
        let result = balanced().build(&chart(), earlier);
        assert_eq!(
            result,
            Err(TransactionError::EffectiveTimeInFuture {
                effective_at: noon(),
                now: earlier,
            })
        );
    }

    #[test]
    fn rejects_effective_time_a_nanosecond_after_now() {
        let earlier = noon() - TimeDelta::nanoseconds(1);
        let error = balanced().build(&chart(), earlier).unwrap_err();
        assert_eq!(
            error.to_string(),
            "effective time 2026-10-16T12:00:00Z is after now (2026-10-16T11:59:59.999999999Z)"
        );
    }

    #[test]
    fn rejects_an_account_not_in_the_chart() {
        let result = builder()
            .debit(ASSET, usd(1_000), [])
            .credit(NOT_IN_CHART, usd(1_000), [])
            .build(&chart(), noon());
        assert_eq!(
            result,
            Err(TransactionError::UnknownAccount {
                entry: 1,
                account: NOT_IN_CHART,
            })
        );
    }

    #[test]
    fn rejects_every_account_when_the_chart_is_empty() {
        let empty = Chart::new([]).unwrap();
        let result = balanced().build(&empty, noon());
        assert_eq!(
            result,
            Err(TransactionError::UnknownAccount {
                entry: 0,
                account: ASSET,
            })
        );
    }

    #[test]
    fn rejects_a_zero_amount() {
        let result = builder()
            .debit(ASSET, usd(0), [])
            .credit(REVENUE, usd(0), [])
            .build(&chart(), noon());
        assert_eq!(
            result,
            Err(TransactionError::NonPositiveAmount {
                entry: 0,
                amount: usd(0),
            })
        );
    }

    #[test]
    fn rejects_a_negative_amount() {
        let result = builder()
            .debit(ASSET, usd(1_000), [])
            .credit(REVENUE, usd(-1_000), [])
            .build(&chart(), noon());
        assert_eq!(
            result,
            Err(TransactionError::NonPositiveAmount {
                entry: 1,
                amount: usd(-1_000),
            })
        );
    }

    #[test]
    fn rejects_two_tags_of_one_kind_on_an_entry() {
        let result = builder()
            .debit(ASSET, usd(1_000), [TestTag::Alpha(42), TestTag::Alpha(43)])
            .credit(REVENUE, usd(1_000), [])
            .build(&chart(), noon());
        assert_eq!(
            result,
            Err(TransactionError::DuplicateTagKind {
                entry: 0,
                kind: "alpha",
            })
        );
    }

    #[test]
    fn rejects_debits_that_overflow() {
        let result = builder()
            .debit(ASSET, usd(i64::MAX), [])
            .debit(ASSET, usd(1), [])
            .credit(REVENUE, usd(1), [])
            .build(&chart(), noon());
        assert_eq!(
            result,
            Err(TransactionError::Overflow { side: Side::Debit })
        );
    }

    #[test]
    fn rejects_credits_that_overflow() {
        let result = builder()
            .debit(ASSET, usd(1), [])
            .credit(REVENUE, usd(i64::MAX), [])
            .credit(REVENUE, usd(1), [])
            .build(&chart(), noon());
        assert_eq!(
            result,
            Err(TransactionError::Overflow { side: Side::Credit })
        );
    }

    #[test]
    fn debits_are_totalled_before_credits() {
        let result = builder()
            .debit(ASSET, usd(i64::MAX), [])
            .debit(ASSET, usd(1), [])
            .credit(REVENUE, usd(i64::MAX), [])
            .credit(REVENUE, usd(1), [])
            .build(&chart(), noon());
        assert_eq!(
            result,
            Err(TransactionError::Overflow { side: Side::Debit })
        );
    }

    #[test]
    fn rejects_unbalanced_entries() {
        let result = builder()
            .debit(ASSET, usd(2_160), [])
            .credit(LIABILITY, usd(2_000), [])
            .build(&chart(), noon());
        assert_eq!(
            result,
            Err(TransactionError::Unbalanced {
                debits: usd(2_160),
                credits: usd(2_000),
            })
        );
    }

    // --- Check order: the first problem wins ---

    #[test]
    fn too_few_entries_is_reported_before_a_future_time() {
        let earlier = noon() - TimeDelta::seconds(1);
        let result = builder().debit(ASSET, usd(1), []).build(&chart(), earlier);
        assert_eq!(result, Err(TransactionError::TooFewEntries { count: 1 }));
    }

    #[test]
    fn a_future_time_is_reported_before_entry_problems() {
        let earlier = noon() - TimeDelta::seconds(1);
        let result = builder()
            .debit(NOT_IN_CHART, usd(1), [])
            .credit(REVENUE, usd(1), [])
            .build(&chart(), earlier);
        assert!(matches!(
            result,
            Err(TransactionError::EffectiveTimeInFuture { .. })
        ));
    }

    #[test]
    fn entries_are_checked_in_order() {
        // Entry 0 has a bad amount; entry 1 has an unknown account.
        let result = builder()
            .debit(ASSET, usd(0), [])
            .credit(NOT_IN_CHART, usd(1), [])
            .build(&chart(), noon());
        assert_eq!(
            result,
            Err(TransactionError::NonPositiveAmount {
                entry: 0,
                amount: usd(0),
            })
        );
    }

    #[test]
    fn within_an_entry_the_amount_is_checked_before_the_tags() {
        let result = builder()
            .debit(ASSET, usd(0), [TestTag::Alpha(1), TestTag::Alpha(2)])
            .credit(REVENUE, usd(1), [])
            .build(&chart(), noon());
        assert_eq!(
            result,
            Err(TransactionError::NonPositiveAmount {
                entry: 0,
                amount: usd(0),
            })
        );
    }

    #[test]
    fn within_an_entry_the_account_is_checked_first() {
        let result = builder()
            .debit(NOT_IN_CHART, usd(0), [TestTag::Alpha(1), TestTag::Alpha(2)])
            .credit(REVENUE, usd(1), [])
            .build(&chart(), noon());
        assert_eq!(
            result,
            Err(TransactionError::UnknownAccount {
                entry: 0,
                account: NOT_IN_CHART,
            })
        );
    }

    #[test]
    fn entry_problems_are_reported_before_an_imbalance() {
        let result = builder()
            .debit(ASSET, usd(1_000), [])
            .credit(REVENUE, usd(-5), [])
            .build(&chart(), noon());
        assert!(matches!(
            result,
            Err(TransactionError::NonPositiveAmount { entry: 1, .. })
        ));
    }

    // --- Messages ---

    #[test]
    fn every_rejection_explains_itself() {
        let cases: [(TransactionError<Usd>, &str); 8] = [
            (
                TransactionError::TooFewEntries { count: 1 },
                "a transaction needs at least 2 entries, got 1",
            ),
            (
                TransactionError::EffectiveTimeInFuture {
                    effective_at: Utc.with_ymd_and_hms(2026, 10, 17, 0, 0, 0).unwrap(),
                    now: noon(),
                },
                "effective time 2026-10-17T00:00:00Z is after now (2026-10-16T12:00:00Z)",
            ),
            (
                TransactionError::UnknownAccount {
                    entry: 2,
                    account: AccountNumber::new(2100),
                },
                "entry 3: account 2100 is not in the chart",
            ),
            (
                TransactionError::NonPositiveAmount {
                    entry: 1,
                    amount: usd(0),
                },
                "entry 2: amount must be positive, got 0.00 USD",
            ),
            (
                TransactionError::DuplicateTagKind {
                    entry: 0,
                    kind: "alpha",
                },
                "entry 1: more than one alpha tag",
            ),
            (
                TransactionError::Overflow { side: Side::Debit },
                "total debits are too large for the ledger to hold",
            ),
            (
                TransactionError::Overflow { side: Side::Credit },
                "total credits are too large for the ledger to hold",
            ),
            (
                TransactionError::Unbalanced {
                    debits: usd(2_160),
                    credits: usd(2_000),
                },
                "debits 21.60 USD ≠ credits 20.00 USD",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    // --- Posting ---

    /// The ledger's chart at posting time: the test chart without the
    /// revenue account.
    fn chart_without_revenue() -> Chart {
        Chart::new([
            Account::new(ASSET, "Asset", AccountType::Asset),
            Account::new(LIABILITY, "Liability", AccountType::Liability),
        ])
        .unwrap()
    }

    const ID: TransactionId = TransactionId::new(7);

    #[test]
    fn post_stamps_the_id() {
        let unposted = balanced().build(&chart(), noon()).unwrap();
        let posted = unposted
            .post(&chart(), TransactionId::new(7), noon())
            .unwrap();
        assert_eq!(posted.id(), TransactionId::new(7));
    }

    #[test]
    fn transactions_reverse_nothing_by_default() {
        let unposted = balanced().build(&chart(), noon()).unwrap();
        assert_eq!(unposted.reverses(), None);
        let posted = unposted.post(&chart(), ID, noon()).unwrap();
        assert_eq!(posted.reverses(), None);
    }

    #[test]
    fn reversing_sets_the_link_and_posting_keeps_it() {
        let unposted = balanced()
            .reversing(TransactionId::new(3))
            .build(&chart(), noon())
            .unwrap();
        assert_eq!(unposted.reverses(), Some(TransactionId::new(3)));
        let posted = unposted.post(&chart(), ID, noon()).unwrap();
        assert_eq!(posted.reverses(), Some(TransactionId::new(3)));
    }

    #[test]
    fn transaction_ids_compare_by_number() {
        assert!(TransactionId::new(1) < TransactionId::new(2));
        assert_eq!(TransactionId::new(5).get(), 5);
    }

    #[test]
    fn post_stamps_the_recorded_time_and_keeps_everything_else() {
        let unposted = balanced().build(&chart(), noon()).unwrap();
        let recorded_at = noon() + TimeDelta::days(2);
        let posted = unposted.clone().post(&chart(), ID, recorded_at).unwrap();
        assert_eq!(posted.recorded_at(), recorded_at);
        assert_eq!(posted.effective_at(), unposted.effective_at());
        assert_eq!(posted.description(), unposted.description());
        assert_eq!(posted.entries(), unposted.entries());
    }

    #[test]
    fn post_accepts_a_recorded_time_equal_to_the_effective_time() {
        let unposted = balanced().build(&chart(), noon()).unwrap();
        assert!(unposted.post(&chart(), ID, noon()).is_ok());
    }

    #[test]
    fn post_rejects_a_recorded_time_before_the_effective_time() {
        let unposted = balanced().build(&chart(), noon()).unwrap();
        let earlier = noon() - TimeDelta::seconds(1);
        assert_eq!(
            unposted.post(&chart(), ID, earlier),
            Err(PostError::EffectiveAfterRecorded {
                effective_at: noon(),
                recorded_at: earlier,
            })
        );
    }

    #[test]
    fn post_rejects_an_account_missing_from_the_ledgers_chart() {
        // Built against a chart with the revenue account; posted into a
        // ledger whose chart doesn't have it.
        let unposted = balanced().build(&chart(), noon()).unwrap();
        assert_eq!(
            unposted.post(&chart_without_revenue(), ID, noon()),
            Err(PostError::UnknownAccount {
                entry: 1,
                account: REVENUE,
            })
        );
    }

    #[test]
    fn post_checks_the_time_before_the_accounts() {
        let unposted = balanced().build(&chart(), noon()).unwrap();
        let earlier = noon() - TimeDelta::seconds(1);
        assert!(matches!(
            unposted.post(&chart_without_revenue(), ID, earlier),
            Err(PostError::EffectiveAfterRecorded { .. })
        ));
    }

    #[test]
    fn post_errors_explain_themselves() {
        let late = PostError::EffectiveAfterRecorded {
            effective_at: Utc.with_ymd_and_hms(2026, 10, 17, 0, 0, 0).unwrap(),
            recorded_at: noon(),
        };
        assert_eq!(
            late.to_string(),
            "effective time 2026-10-17T00:00:00Z is after recorded time (2026-10-16T12:00:00Z)"
        );
        let unknown = PostError::UnknownAccount {
            entry: 1,
            account: AccountNumber::new(3),
        };
        assert_eq!(
            unknown.to_string(),
            "entry 2: account 3 is not in the ledger's chart"
        );
    }

    // --- Rules that must always hold ---

    /// At most one thing wrong with a valid transaction. `Index` picks which
    /// entry, whatever the number of entries turns out to be.
    #[derive(Clone, Debug)]
    enum Corruption {
        None,
        KeepOnlyOneEntry,
        UnknownAccount(prop::sample::Index),
        NonPositiveAmount(prop::sample::Index, i64),
        RepeatedTagKind(prop::sample::Index),
        AddToOneAmount(prop::sample::Index, i64),
        SetOneAmountToTheMaximum(prop::sample::Index),
        EffectiveTimeAfterNow(i64),
    }

    fn any_corruption() -> impl Strategy<Value = Corruption> {
        prop_oneof![
            Just(Corruption::None),
            Just(Corruption::KeepOnlyOneEntry),
            any::<prop::sample::Index>().prop_map(Corruption::UnknownAccount),
            (any::<prop::sample::Index>(), -1_000_i64..=0)
                .prop_map(|(i, cents)| Corruption::NonPositiveAmount(i, cents)),
            any::<prop::sample::Index>().prop_map(Corruption::RepeatedTagKind),
            (any::<prop::sample::Index>(), 1_i64..=1_000)
                .prop_map(|(i, cents)| Corruption::AddToOneAmount(i, cents)),
            any::<prop::sample::Index>().prop_map(Corruption::SetOneAmountToTheMaximum),
            // Up to a year late, to the second.
            (1_i64..=31_536_000).prop_map(Corruption::EffectiveTimeAfterNow),
        ]
    }

    /// Applies the corruption and returns the error `build` must report, or
    /// `None` if the transaction is still valid.
    fn corrupt(
        entries: &mut Vec<TestEntry>,
        effective_at: &mut DateTime<Utc>,
        corruption: &Corruption,
    ) -> Option<TransactionError<Usd>> {
        match corruption {
            Corruption::None => None,
            Corruption::KeepOnlyOneEntry => {
                entries.truncate(1);
                Some(TransactionError::TooFewEntries { count: 1 })
            }
            Corruption::UnknownAccount(index) => {
                let entry = index.index(entries.len());
                entries[entry].0 = NOT_IN_CHART;
                Some(TransactionError::UnknownAccount {
                    entry,
                    account: NOT_IN_CHART,
                })
            }
            Corruption::NonPositiveAmount(index, cents) => {
                let entry = index.index(entries.len());
                entries[entry].2 = *cents;
                Some(TransactionError::NonPositiveAmount {
                    entry,
                    amount: usd(*cents),
                })
            }
            Corruption::RepeatedTagKind(index) => {
                let entry = index.index(entries.len());
                // The repeated kind isn't next to its twin, so a check that
                // only compares neighbours would miss it.
                entries[entry].3 = vec![TestTag::Alpha(1), TestTag::Beta(1), TestTag::Alpha(2)];
                Some(TransactionError::DuplicateTagKind {
                    entry,
                    kind: "alpha",
                })
            }
            Corruption::AddToOneAmount(index, cents) => {
                let entry = index.index(entries.len());
                entries[entry].2 += cents;
                let total = |side: Side| -> i64 {
                    entries.iter().filter(|e| e.1 == side).map(|e| e.2).sum()
                };
                Some(TransactionError::Unbalanced {
                    debits: usd(total(Side::Debit)),
                    credits: usd(total(Side::Credit)),
                })
            }
            Corruption::SetOneAmountToTheMaximum(index) => {
                let entry = index.index(entries.len());
                entries[entry].2 = i64::MAX;
                // The exact totals, in `i128` where they can't overflow.
                let total = |side: Side| -> i128 {
                    entries
                        .iter()
                        .filter(|e| e.1 == side)
                        .map(|e| i128::from(e.2))
                        .sum()
                };
                let (debits, credits) = (total(Side::Debit), total(Side::Credit));
                let max = i128::from(i64::MAX);
                Some(if debits > max {
                    TransactionError::Overflow { side: Side::Debit }
                } else if credits > max {
                    TransactionError::Overflow { side: Side::Credit }
                } else {
                    TransactionError::Unbalanced {
                        debits: usd(i64::try_from(debits).unwrap()),
                        credits: usd(i64::try_from(credits).unwrap()),
                    }
                })
            }
            Corruption::EffectiveTimeAfterNow(seconds) => {
                *effective_at = noon() + TimeDelta::seconds(*seconds);
                Some(TransactionError::EffectiveTimeInFuture {
                    effective_at: *effective_at,
                    now: noon(),
                })
            }
        }
    }

    /// Builds the entries into a transaction effective at `effective_at`,
    /// with `now` at noon.
    fn build_entries(
        entries: &[TestEntry],
        effective_at: DateTime<Utc>,
    ) -> Result<UnpostedTransaction<Usd, TestTag>, TransactionError<Usd>> {
        let start = UnpostedTransaction::builder(effective_at, "Test");
        entries
            .iter()
            .cloned()
            .fold(start, |b, (account, side, cents, tags)| match side {
                Side::Debit => b.debit(account, usd(cents), tags),
                Side::Credit => b.credit(account, usd(cents), tags),
            })
            .build(&chart(), noon())
    }

    proptest! {
        /// A transaction builds only if it follows every rule, and a broken
        /// one is rejected for the rule it breaks. Starts from a valid
        /// transaction and breaks at most one rule, so both outcomes are
        /// common. When it builds: at least two entries, every account in the
        /// chart, every amount positive, no repeated tag kind on an entry, and
        /// debits equal to credits (summed in `i128`, where they can't
        /// overflow).
        #[test]
        fn builds_only_valid_transactions_and_names_what_is_wrong(
            mut entries in valid_entries(),
            corruption in any_corruption(),
        ) {
            let mut effective_at = noon();
            let expected = corrupt(&mut entries, &mut effective_at, &corruption);
            let result = build_entries(&entries, effective_at);
            match expected {
                Some(error) => prop_assert_eq!(result, Err(error)),
                None => {
                    let t = result.map_err(|e| TestCaseError::fail(e.to_string()))?;
                    let chart = chart();
                    prop_assert!(t.entries().len() >= 2);
                    prop_assert!(t.entries().iter().all(|e| chart.get(e.account()).is_some()));
                    prop_assert!(t.entries().iter().all(|e| e.amount().minor_units() > 0));
                    for e in t.entries() {
                        let kinds: HashSet<&str> = e.tags().iter().map(Tag::kind).collect();
                        prop_assert_eq!(kinds.len(), e.tags().len());
                    }
                    let sum = |side: Side| -> i128 {
                        t.entries()
                            .iter()
                            .filter(|e| e.side() == side)
                            .map(|e| i128::from(e.amount().minor_units()))
                            .sum()
                    };
                    prop_assert_eq!(sum(Side::Debit), sum(Side::Credit));
                }
            }
        }
    }
}
