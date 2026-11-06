//! The store's cache of accounts it has already recorded and checked.

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};

use crate::account::AccountNumber;

/// Accounts this store has seen committed with the chart's type and contra
/// flag; later saves skip recording and checking them. Safe because recorded
/// rows never change. Lost on restart, which just means one more check.
///
/// Every method is synchronous, so the lock can never be held across an
/// `.await`.
pub(super) struct ConfirmedAccounts(Mutex<HashSet<AccountNumber>>);

impl ConfirmedAccounts {
    pub(super) fn new() -> Self {
        Self(Mutex::new(HashSet::new()))
    }

    /// The accounts in `accounts` not yet confirmed, in the order given.
    pub(super) fn unconfirmed(&self, accounts: &[AccountNumber]) -> Vec<AccountNumber> {
        let confirmed = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        accounts
            .iter()
            .copied()
            .filter(|account| !confirmed.contains(account))
            .collect()
    }

    /// Marks `accounts` confirmed. Call it only after the database
    /// transaction that recorded them has committed: marking an account
    /// before a rollback would make later saves skip recording it, and the
    /// entries' foreign key would then fail.
    pub(super) fn mark_committed(&self, accounts: &[AccountNumber]) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(accounts.iter().copied());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconfirmed_skips_only_the_marked_accounts() {
        let (a, b, c) = (
            AccountNumber::new(1),
            AccountNumber::new(2),
            AccountNumber::new(3),
        );
        let confirmed = ConfirmedAccounts::new();
        assert_eq!(confirmed.unconfirmed(&[a, b, c]), vec![a, b, c]);

        confirmed.mark_committed(&[a, b]);
        assert_eq!(confirmed.unconfirmed(&[a, b, c]), vec![c]);
    }
}
