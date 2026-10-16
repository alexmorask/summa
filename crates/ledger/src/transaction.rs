//! Transactions: two or more entries that balance. A transaction is built and
//! checked as an [`UnpostedTransaction`]; only the ledger posts it.

use std::collections::HashSet;
use std::fmt;

use chrono::{DateTime, SecondsFormat, Utc};

use crate::account::{AccountNumber, Side};
use crate::chart::Chart;
use crate::money::{Currency, Money, MoneyError};
use crate::tag::Tag;

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

/// A transaction that passed every check but hasn't been posted: it has an
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

    /// Posts the transaction: stamps the time the ledger recorded it.
    ///
    /// This is the authoritative time check: nothing may be recorded as
    /// having happened after it was written down, so `effective_at` must not
    /// be after `recorded_at`. `build`'s check against `now` is early feedback
    /// for the caller.
    ///
    /// Private to the ledger crate, so no other crate can post or choose a
    /// recorded time. Ledger storage calls it from M2 (SUM-9); until then only
    /// tests do.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "called by ledger storage from M2 (SUM-9)")
    )]
    pub(crate) fn post(
        self,
        recorded_at: DateTime<Utc>,
    ) -> Result<PostedTransaction<C, T>, TransactionError<C>> {
        if self.effective_at > recorded_at {
            return Err(TransactionError::EffectiveAfterRecorded {
                effective_at: self.effective_at,
                recorded_at,
            });
        }
        Ok(PostedTransaction {
            transaction: self,
            recorded_at,
        })
    }
}

/// A transaction the ledger has posted: everything the unposted transaction
/// had, plus the time it was recorded. Only the ledger can create one.
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
/// let _posted = unposted.post(now);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostedTransaction<C: Currency, T: Tag> {
    transaction: UnpostedTransaction<C, T>,
    recorded_at: DateTime<Utc>,
}

impl<C: Currency, T: Tag> PostedTransaction<C, T> {
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
}

impl<C: Currency, T: Tag> TransactionBuilder<C, T> {
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

    /// Checks every rule and returns the transaction, or the first problem
    /// found. The checks run in a fixed order: whole-transaction structure,
    /// then each entry in the order added, then the totals. An entry problem
    /// often explains a bad total, and totals only mean something once every
    /// amount is positive.
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
        })
    }
}

/// The first tag kind that appears more than once, if any.
fn repeated_kind<T: Tag>(tags: &[T]) -> Option<&'static str> {
    let mut seen = HashSet::new();
    tags.iter().map(Tag::kind).find(|&kind| !seen.insert(kind))
}

/// The sum of the amounts on one side, or `Overflow` if it doesn't fit.
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
        .map_err(|MoneyError::Overflow| TransactionError::Overflow)
}

/// Why a transaction was rejected. `entry` is the entry's 0-based position;
/// messages show it 1-based.
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
    /// Total debits or total credits don't fit in an `i64` of minor units.
    Overflow,
    /// Total debits don't equal total credits.
    Unbalanced { debits: Money<C>, credits: Money<C> },
    /// `post` was given a recorded time before the effective time.
    EffectiveAfterRecorded {
        effective_at: DateTime<Utc>,
        recorded_at: DateTime<Utc>,
    },
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
            TransactionError::Overflow => {
                f.write_str("total debits or credits are too large for the ledger to hold")
            }
            TransactionError::Unbalanced { debits, credits } => {
                write!(f, "debits {debits} ≠ credits {credits}")
            }
            TransactionError::EffectiveAfterRecorded {
                effective_at,
                recorded_at,
            } => write!(
                f,
                "effective time {} is after recorded time ({})",
                utc(*effective_at),
                utc(*recorded_at)
            ),
        }
    }
}

impl<C: Currency> std::error::Error for TransactionError<C> {}

/// RFC 3339 in UTC with a `Z`, showing fractional seconds only when present.
fn utc(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{Account, AccountType};
    use crate::money::Usd;
    use chrono::{TimeDelta, TimeZone};

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum TestTag {
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

    const ASSET: AccountNumber = AccountNumber::new(1);
    const LIABILITY: AccountNumber = AccountNumber::new(2);
    const REVENUE: AccountNumber = AccountNumber::new(3);
    const NOT_IN_CHART: AccountNumber = AccountNumber::new(99);

    fn chart() -> Chart {
        Chart::new([
            Account::new(ASSET, "Asset", AccountType::Asset),
            Account::new(LIABILITY, "Liability", AccountType::Liability),
            Account::new(REVENUE, "Revenue", AccountType::Revenue),
        ])
        .unwrap()
    }

    fn usd(cents: i64) -> Money<Usd> {
        Money::from_minor_units(cents)
    }

    /// The fixed "now" every test uses: 2026-10-16 12:00:00 UTC.
    fn noon() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap()
    }

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
    fn rejects_totals_that_overflow() {
        let result = builder()
            .debit(ASSET, usd(i64::MAX), [])
            .debit(ASSET, usd(1), [])
            .credit(REVENUE, usd(1), [])
            .build(&chart(), noon());
        assert_eq!(result, Err(TransactionError::Overflow));
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
        let cases: [(TransactionError<Usd>, &str); 7] = [
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
                    kind: "customer",
                },
                "entry 1: more than one customer tag",
            ),
            (
                TransactionError::Overflow,
                "total debits or credits are too large for the ledger to hold",
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

    #[test]
    fn post_stamps_the_recorded_time_and_keeps_everything_else() {
        let unposted = balanced().build(&chart(), noon()).unwrap();
        let recorded_at = noon() + TimeDelta::days(2);
        let posted = unposted.clone().post(recorded_at).unwrap();
        assert_eq!(posted.recorded_at(), recorded_at);
        assert_eq!(posted.effective_at(), unposted.effective_at());
        assert_eq!(posted.description(), unposted.description());
        assert_eq!(posted.entries(), unposted.entries());
    }

    #[test]
    fn post_accepts_a_recorded_time_equal_to_the_effective_time() {
        let unposted = balanced().build(&chart(), noon()).unwrap();
        assert!(unposted.post(noon()).is_ok());
    }

    #[test]
    fn post_rejects_a_recorded_time_before_the_effective_time() {
        let unposted = balanced().build(&chart(), noon()).unwrap();
        let earlier = noon() - TimeDelta::seconds(1);
        assert_eq!(
            unposted.post(earlier),
            Err(TransactionError::EffectiveAfterRecorded {
                effective_at: noon(),
                recorded_at: earlier,
            })
        );
    }

    #[test]
    fn effective_after_recorded_explains_itself() {
        let error = TransactionError::<Usd>::EffectiveAfterRecorded {
            effective_at: Utc.with_ymd_and_hms(2026, 10, 17, 0, 0, 0).unwrap(),
            recorded_at: noon(),
        };
        assert_eq!(
            error.to_string(),
            "effective time 2026-10-17T00:00:00Z is after recorded time (2026-10-16T12:00:00Z)"
        );
    }
}
