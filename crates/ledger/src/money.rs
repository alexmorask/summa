//! Money: a whole number of minor units (cents, for USD) in a currency fixed
//! by the type.

use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;

/// A currency, used as a type parameter so amounts in different currencies
/// can't be mixed (ADR 0004).
///
/// The supertraits guarantee every `Money<C>`, and every type holding one, gets
/// its derived traits: `#[derive]` only implements a trait for `Money<C>` when
/// `C` has it too, even though `C` only appears in `PhantomData`.
pub trait Currency: Copy + Clone + fmt::Debug + PartialEq + Eq + PartialOrd + Ord + Hash {
    /// The ISO 4217 code, like `"USD"`.
    const CODE: &'static str;

    /// How many digits of minor units make up one major unit: 2 for US dollars
    /// (100 cents), 0 for the yen. ISO 4217 calls this the "minor unit".
    ///
    /// At most 19, so that 10 to that power (the minor units in one major
    /// unit) fits in a `u64`. Creating an amount in a currency that declares
    /// more is a compile error:
    ///
    /// ```
    /// use ledger::{Currency, Money};
    ///
    /// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    /// struct Fine;
    ///
    /// impl Currency for Fine {
    ///     const CODE: &'static str = "XFN";
    ///     const DECIMAL_PLACES: u32 = 19;
    /// }
    ///
    /// let one = Money::<Fine>::from_minor_units(1);
    /// assert_eq!(one.to_string(), "0.0000000000000000001 XFN");
    /// ```
    ///
    /// ```compile_fail
    /// use ledger::{Currency, Money};
    ///
    /// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    /// struct TooFine;
    ///
    /// impl Currency for TooFine {
    ///     const CODE: &'static str = "XTF";
    ///     const DECIMAL_PLACES: u32 = 20;
    /// }
    ///
    /// // Creating the amount is enough to fail; it never has to be displayed.
    /// let _one = Money::<TooFine>::from_minor_units(1);
    /// ```
    const DECIMAL_PLACES: u32;
}

/// US dollars, the only currency in v1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Usd;

impl Currency for Usd {
    const CODE: &'static str = "USD";
    const DECIMAL_PLACES: u32 = 2;
}

/// An amount of money: a whole number of minor units in currency `C`, such as
/// cents for US dollars.
///
/// It may be negative, because balances can be. `Money` itself doesn't require a
/// positive amount, even though amounts on entries must be positive. There is
/// no `+` operator: amounts are added with [`Money::checked_add`], which
/// rejects overflow.
///
/// ```
/// use ledger::{Money, Usd};
///
/// // $20.00 + $1.60 = $21.60
/// let plan = Money::<Usd>::from_minor_units(2_000);
/// let tax = Money::<Usd>::from_minor_units(160);
/// assert_eq!(plan.checked_add(tax), Ok(Money::from_minor_units(2_160)));
/// ```
///
/// The same amounts can't be added with `+`, so no caller can skip the
/// overflow check:
///
/// ```compile_fail
/// use ledger::{Money, Usd};
///
/// let plan = Money::<Usd>::from_minor_units(2_000);
/// let tax = Money::<Usd>::from_minor_units(160);
/// let _total = plan + tax;
/// ```
///
/// Adding amounts in the same currency compiles:
///
/// ```
/// use ledger::{Currency, Money};
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// struct Eur;
///
/// impl Currency for Eur {
///     const CODE: &'static str = "EUR";
///     const DECIMAL_PLACES: u32 = 2;
/// }
///
/// let a = Money::<Eur>::from_minor_units(100);
/// let b = Money::<Eur>::from_minor_units(250);
/// let _sum = a.checked_add(b);
/// ```
///
/// The same code with different currencies doesn't:
///
/// ```compile_fail
/// use ledger::{Currency, Money, Usd};
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// struct Eur;
///
/// impl Currency for Eur {
///     const CODE: &'static str = "EUR";
///     const DECIMAL_PLACES: u32 = 2;
/// }
///
/// let a = Money::<Usd>::from_minor_units(100);
/// let b = Money::<Eur>::from_minor_units(250);
/// let _sum = a.checked_add(b);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Money<C: Currency> {
    minor_units: i64,
    currency: PhantomData<C>,
}

impl<C: Currency> Money<C> {
    /// An amount of `minor_units` in currency `C`.
    pub const fn from_minor_units(minor_units: i64) -> Self {
        // Checked when the compiler generates this code for a specific `C`.
        // Every amount is created here, so a currency with too many decimal
        // places fails the build, instead of overflowing `10_u64.pow` when an
        // amount is displayed.
        const {
            assert!(
                C::DECIMAL_PLACES <= 19,
                "a currency can have at most 19 decimal places"
            )
        };
        Self {
            minor_units,
            currency: PhantomData,
        }
    }

    /// The amount in minor units.
    pub const fn minor_units(self) -> i64 {
        self.minor_units
    }

    /// Adds two amounts, or returns [`MoneyError::Overflow`] if the exact sum
    /// doesn't fit in an `i64` number of minor units.
    pub fn checked_add(self, other: Self) -> Result<Self, MoneyError> {
        self.minor_units
            .checked_add(other.minor_units)
            .map(Self::from_minor_units)
            .ok_or(MoneyError::Overflow)
    }
}

/// Prints `Money(2160 USD)`: the minor units and the currency code, without
/// the `PhantomData` marker a derived `Debug` would show.
impl<C: Currency> fmt::Debug for Money<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Money({} {})", self.minor_units, C::CODE)
    }
}

/// Prints the amount in major units with the currency code, like `21.60 USD`
/// or `-0.05 USD`. One implementation serves every currency: the code and the
/// number of decimal places come from `C`.
impl<C: Currency> fmt::Display for Money<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The sign is written separately: for -5 cents the whole part is 0,
        // and an integer can't be -0. `unsigned_abs` (not `abs`) because
        // `i64::MIN` has no positive `i64` counterpart.
        let sign = if self.minor_units < 0 { "-" } else { "" };
        let units = self.minor_units.unsigned_abs();
        let code = C::CODE;
        if C::DECIMAL_PLACES == 0 {
            return write!(f, "{sign}{units} {code}");
        }
        let scale = 10_u64.pow(C::DECIMAL_PLACES);
        let whole = units / scale;
        let fraction = units % scale;
        let width = C::DECIMAL_PLACES as usize;
        write!(f, "{sign}{whole}.{fraction:0width$} {code}")
    }
}

/// Why an operation on [`Money`] was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoneyError {
    /// The exact result doesn't fit in an `i64` number of minor units.
    Overflow,
}

impl fmt::Display for MoneyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoneyError::Overflow => {
                f.write_str("amount is too large or too small for the ledger to hold")
            }
        }
    }
}

impl std::error::Error for MoneyError {}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn usd(cents: i64) -> Money<Usd> {
        Money::from_minor_units(cents)
    }

    #[test]
    fn stores_whole_minor_units() {
        // $21.60 is 2,160 cents, the minor unit of USD.
        assert_eq!(usd(2_160).minor_units(), 2_160);
    }

    #[test]
    fn usd_code_is_usd() {
        assert_eq!(Usd::CODE, "USD");
    }

    #[test]
    fn debug_shows_minor_units_and_currency_code() {
        assert_eq!(format!("{:?}", usd(2_160)), "Money(2160 USD)");
        assert_eq!(format!("{:?}", usd(-500)), "Money(-500 USD)");
    }

    /// A currency with no minor units, like the yen.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    struct Jpy;

    impl Currency for Jpy {
        const CODE: &'static str = "JPY";
        const DECIMAL_PLACES: u32 = 0;
    }

    #[test]
    fn usd_has_two_decimal_places() {
        assert_eq!(Usd::DECIMAL_PLACES, 2);
    }

    #[test]
    fn displays_whole_and_fractional_units_with_the_code() {
        assert_eq!(usd(2_160).to_string(), "21.60 USD");
        assert_eq!(usd(5).to_string(), "0.05 USD");
        assert_eq!(usd(0).to_string(), "0.00 USD");
    }

    #[test]
    fn displays_negative_amounts() {
        assert_eq!(usd(-500).to_string(), "-5.00 USD");
        // The whole part is 0 and there's no integer −0, so the sign has to
        // be written separately or it's lost.
        assert_eq!(usd(-5).to_string(), "-0.05 USD");
    }

    #[test]
    fn displays_the_limits_without_panicking() {
        assert_eq!(usd(i64::MAX).to_string(), "92233720368547758.07 USD");
        assert_eq!(usd(i64::MIN).to_string(), "-92233720368547758.08 USD");
    }

    #[test]
    fn displays_currencies_without_minor_units() {
        assert_eq!(
            Money::<Jpy>::from_minor_units(2_160).to_string(),
            "2160 JPY"
        );
        assert_eq!(Money::<Jpy>::from_minor_units(-5).to_string(), "-5 JPY");
    }

    #[test]
    fn adds_amounts_exactly() {
        // $20.00 + $1.60 = $21.60
        assert_eq!(usd(2_000).checked_add(usd(160)), Ok(usd(2_160)));
    }

    #[test]
    fn adds_negative_and_zero_amounts() {
        // −$5.00 + $2.00 = −$3.00: balances can go below zero.
        assert_eq!(usd(-500).checked_add(usd(200)), Ok(usd(-300)));
        // $0.00 + $21.60 = $21.60
        assert_eq!(usd(0).checked_add(usd(2_160)), Ok(usd(2_160)));
    }

    #[test]
    fn sums_exactly_at_the_limits_are_accepted() {
        assert_eq!(usd(i64::MAX - 1).checked_add(usd(1)), Ok(usd(i64::MAX)));
        assert_eq!(usd(i64::MIN + 1).checked_add(usd(-1)), Ok(usd(i64::MIN)));
        assert_eq!(usd(i64::MAX).checked_add(usd(i64::MIN)), Ok(usd(-1)));
    }

    #[test]
    fn rejects_overflow_above_the_maximum() {
        assert_eq!(usd(i64::MAX).checked_add(usd(1)), Err(MoneyError::Overflow));
    }

    #[test]
    fn rejects_overflow_below_the_minimum() {
        assert_eq!(
            usd(i64::MIN).checked_add(usd(-1)),
            Err(MoneyError::Overflow)
        );
    }

    #[test]
    fn overflow_error_explains_itself() {
        assert_eq!(
            MoneyError::Overflow.to_string(),
            "amount is too large or too small for the ledger to hold"
        );
    }

    proptest! {
        /// Adding never loses or invents a cent: the result is the exact sum,
        /// computed in `i128` where it can't overflow, or an error.
        #[test]
        fn adding_never_loses_or_invents_a_cent(a in any::<i64>(), b in any::<i64>()) {
            let exact = i128::from(a) + i128::from(b);
            let result = usd(a).checked_add(usd(b));
            match i64::try_from(exact) {
                Ok(sum) => prop_assert_eq!(result, Ok(usd(sum))),
                Err(_) => prop_assert_eq!(result, Err(MoneyError::Overflow)),
            }
        }
    }
}
