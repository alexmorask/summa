//! Accounts: the buckets of a chart of accounts, and the properties derived
//! from their type.

/// The two sides of an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Debit,
    Credit,
}

impl Side {
    /// The other side.
    pub fn opposite(self) -> Side {
        match self {
            Side::Debit => Side::Credit,
            Side::Credit => Side::Debit,
        }
    }
}

/// The five account types, from the accounting equation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccountType {
    Asset,
    Liability,
    Equity,
    Revenue,
    Expense,
}

impl AccountType {
    /// The side that makes an account of this type grow.
    fn normal_side(self) -> Side {
        match self {
            AccountType::Asset | AccountType::Expense => Side::Debit,
            AccountType::Liability | AccountType::Equity | AccountType::Revenue => Side::Credit,
        }
    }
}

/// Whether an account's balance carries forward forever or measures one period.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Permanence {
    Permanent,
    Temporary,
}

/// An account number, like 2000. The numbers are for humans and carry no
/// meaning in the ledger's rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AccountNumber(u32);

impl AccountNumber {
    pub const fn new(number: u32) -> Self {
        Self(number)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// One account in the chart of accounts.
///
/// Normal side and permanence are derived from the type and contra flag,
/// never stored, so they can't disagree with them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Account {
    number: AccountNumber,
    name: &'static str,
    account_type: AccountType,
    contra: bool,
}

impl Account {
    pub const fn new(
        number: AccountNumber,
        name: &'static str,
        account_type: AccountType,
        contra: bool,
    ) -> Self {
        Self {
            number,
            name,
            account_type,
            contra,
        }
    }

    pub const fn number(&self) -> AccountNumber {
        self.number
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub const fn account_type(&self) -> AccountType {
        self.account_type
    }

    pub const fn is_contra(&self) -> bool {
        self.contra
    }

    /// The side that makes this account grow: its type's normal side, or the
    /// opposite for a contra account.
    pub fn normal_side(&self) -> Side {
        let side = self.account_type.normal_side();
        if self.contra { side.opposite() } else { side }
    }

    /// Assets, liabilities and equity are permanent; revenue and expenses are
    /// temporary. Permanence follows the account type and ignores the contra
    /// flag. The one exception would be dividends tracked in a separate
    /// temporary account instead of debited to Retained Earnings directly,
    /// which would need permanence as its own setting. Summa has no such
    /// account.
    pub fn permanence(&self) -> Permanence {
        match self.account_type {
            AccountType::Asset | AccountType::Liability | AccountType::Equity => {
                Permanence::Permanent
            }
            AccountType::Revenue | AccountType::Expense => Permanence::Temporary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const ALL_TYPES: [AccountType; 5] = [
        AccountType::Asset,
        AccountType::Liability,
        AccountType::Equity,
        AccountType::Revenue,
        AccountType::Expense,
    ];

    fn account(account_type: AccountType, contra: bool) -> Account {
        Account::new(AccountNumber::new(1), "Test", account_type, contra)
    }

    /// Normal sides from the Chart of Accounts, written out independently of
    /// the code under test.
    fn chart_normal_side(account_type: AccountType) -> Side {
        match account_type {
            AccountType::Asset => Side::Debit,
            AccountType::Liability => Side::Credit,
            AccountType::Equity => Side::Credit,
            AccountType::Revenue => Side::Credit,
            AccountType::Expense => Side::Debit,
        }
    }

    #[test]
    fn opposite_flips_the_side() {
        assert_eq!(Side::Debit.opposite(), Side::Credit);
        assert_eq!(Side::Credit.opposite(), Side::Debit);
    }

    #[test]
    fn getters_return_what_was_given() {
        let a = Account::new(
            AccountNumber::new(42),
            "Test Contra",
            AccountType::Revenue,
            true,
        );
        assert_eq!(a.number().get(), 42);
        assert_eq!(a.name(), "Test Contra");
        assert_eq!(a.account_type(), AccountType::Revenue);
        assert!(a.is_contra());
    }

    #[test]
    fn assets_are_debit_normal() {
        assert_eq!(
            account(AccountType::Asset, false).normal_side(),
            Side::Debit
        );
    }

    #[test]
    fn liabilities_are_credit_normal() {
        assert_eq!(
            account(AccountType::Liability, false).normal_side(),
            Side::Credit
        );
    }

    #[test]
    fn equity_is_credit_normal() {
        assert_eq!(
            account(AccountType::Equity, false).normal_side(),
            Side::Credit
        );
    }

    #[test]
    fn revenue_is_credit_normal() {
        assert_eq!(
            account(AccountType::Revenue, false).normal_side(),
            Side::Credit
        );
    }

    #[test]
    fn expenses_are_debit_normal() {
        assert_eq!(
            account(AccountType::Expense, false).normal_side(),
            Side::Debit
        );
    }

    #[test]
    fn contra_revenue_is_debit_normal() {
        assert_eq!(
            account(AccountType::Revenue, true).normal_side(),
            Side::Debit
        );
    }

    #[test]
    fn contra_asset_is_credit_normal() {
        assert_eq!(
            account(AccountType::Asset, true).normal_side(),
            Side::Credit
        );
    }

    #[test]
    fn balance_sheet_types_are_permanent() {
        for t in [
            AccountType::Asset,
            AccountType::Liability,
            AccountType::Equity,
        ] {
            assert_eq!(
                account(t, false).permanence(),
                Permanence::Permanent,
                "{t:?}"
            );
        }
    }

    #[test]
    fn income_statement_types_are_temporary() {
        for t in [AccountType::Revenue, AccountType::Expense] {
            assert_eq!(
                account(t, false).permanence(),
                Permanence::Temporary,
                "{t:?}"
            );
        }
    }

    #[test]
    fn contra_flag_does_not_change_permanence() {
        assert_eq!(
            account(AccountType::Revenue, true).permanence(),
            Permanence::Temporary
        );
        assert_eq!(
            account(AccountType::Asset, true).permanence(),
            Permanence::Permanent
        );
    }

    proptest! {
        /// An account's normal side always follows from its type and contra flag.
        #[test]
        fn normal_side_follows_type_and_contra_flag(
            account_type in prop::sample::select(ALL_TYPES.to_vec()),
            number in any::<u32>(),
            name in prop::sample::select(vec!["A", "Test Contra", "Test"]),
        ) {
            let plain = Account::new(AccountNumber::new(number), name, account_type, false);
            let contra = Account::new(AccountNumber::new(number), name, account_type, true);

            prop_assert_eq!(plain.normal_side(), chart_normal_side(account_type));
            prop_assert_eq!(contra.normal_side(), plain.normal_side().opposite());
            prop_assert_eq!(contra.permanence(), plain.permanence());
        }
    }
}
