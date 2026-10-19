//! The chart of accounts: the business's accounts, passed into the ledger and
//! keyed by number. The ledger defines what an account is; the business
//! supplies which accounts exist (ADR 0005).

use std::collections::BTreeMap;
use std::fmt;

use crate::account::{Account, AccountNumber};

/// The accounts a transaction's entries may use, keyed by number. Holds the
/// only copy of each account: entries refer to accounts by number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chart {
    accounts: BTreeMap<AccountNumber, Account>,
}

impl Chart {
    /// A chart of `accounts`, or [`ChartError::DuplicateNumber`] if two share
    /// a number. Account numbers are an account's identity, so they must be
    /// unique.
    pub fn new(accounts: impl IntoIterator<Item = Account>) -> Result<Self, ChartError> {
        let mut by_number = BTreeMap::new();
        for account in accounts {
            let number = account.number();
            if by_number.insert(number, account).is_some() {
                return Err(ChartError::DuplicateNumber { number });
            }
        }
        Ok(Self {
            accounts: by_number,
        })
    }

    /// The account with this number, if the chart has one.
    pub fn get(&self, number: AccountNumber) -> Option<&Account> {
        self.accounts.get(&number)
    }
}

/// Why a chart was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartError {
    /// Two accounts have the same number.
    DuplicateNumber { number: AccountNumber },
}

impl fmt::Display for ChartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChartError::DuplicateNumber { number } => write!(
                f,
                "account number {} appears more than once in the chart",
                number.get()
            ),
        }
    }
}

impl std::error::Error for ChartError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{AccountNumber, AccountType};

    fn account(number: u32, name: &'static str) -> Account {
        Account::new(AccountNumber::new(number), name, AccountType::Asset)
    }

    #[test]
    fn finds_accounts_by_number() {
        let chart = Chart::new([account(1, "First"), account(2, "Second")]).unwrap();
        assert_eq!(
            chart.get(AccountNumber::new(2)),
            Some(&account(2, "Second"))
        );
    }

    #[test]
    fn unknown_numbers_are_not_found() {
        let chart = Chart::new([account(1, "First")]).unwrap();
        assert_eq!(chart.get(AccountNumber::new(9)), None);
    }

    #[test]
    fn an_empty_chart_is_allowed() {
        let chart = Chart::new([]).unwrap();
        assert_eq!(chart.get(AccountNumber::new(1)), None);
    }

    #[test]
    fn rejects_two_accounts_with_the_same_number() {
        let result = Chart::new([account(1, "First"), account(1, "Also First")]);
        assert_eq!(
            result,
            Err(ChartError::DuplicateNumber {
                number: AccountNumber::new(1)
            })
        );
    }

    #[test]
    fn duplicate_number_error_explains_itself() {
        let error = ChartError::DuplicateNumber {
            number: AccountNumber::new(1000),
        };
        assert_eq!(
            error.to_string(),
            "account number 1000 appears more than once in the chart"
        );
    }
}
