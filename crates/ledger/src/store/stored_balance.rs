//! Reading a balance from the totals the balance worker keeps.

use super::error::StoredBalanceError;
use super::netting::Totals;
use super::{Store, StoredTag};
use crate::account::AccountNumber;
use crate::balance::BalanceError;
use crate::money::{Money, MoneyError, Usd};

impl<T: StoredTag> Store<T> {
    /// The stored balance of `account`, or of `account` restricted to `tag`.
    /// It is the same number `balance` gives over every transaction the
    /// worker has processed, with no cutoffs. An account or tag with no
    /// stored row reads as zero. An account missing from the chart is
    /// `Balance(UnknownAccount)`.
    ///
    /// It can lag behind the log (see `BalanceWorker::lag`), so never use it
    /// to decide whether a spend is allowed. A stopped or held-back worker
    /// leaves values stale, and during a rebuild reads are unreliable until
    /// it finishes: balances fall to zero and rise as the replay catches up.
    pub async fn stored_balance(
        &self,
        account: AccountNumber,
        tag: Option<&T>,
    ) -> Result<Money<Usd>, StoredBalanceError> {
        let normal_side = self
            .chart
            .get(account)
            .ok_or(StoredBalanceError::Balance(BalanceError::UnknownAccount {
                account,
            }))?
            .normal_side();
        let number = i64::from(account.get());
        let (debit, credit) = match tag {
            None => sqlx::query!(
                "SELECT debit_total, credit_total FROM account_balances WHERE account = $1",
                number
            )
            .fetch_optional(&self.pool)
            .await
            .map_err(StoredBalanceError::Unavailable)?
            .map(|row| (row.debit_total, row.credit_total)),
            Some(tag) => sqlx::query!(
                "SELECT debit_total, credit_total FROM tag_balances \
                 WHERE account = $1 AND kind = $2 AND value = $3",
                number,
                tag.kind(),
                tag.value()
            )
            .fetch_optional(&self.pool)
            .await
            .map_err(StoredBalanceError::Unavailable)?
            .map(|row| (row.debit_total, row.credit_total)),
        }
        .unwrap_or((0, 0));
        Totals::from_minor_units(debit, credit)
            .net(normal_side)
            .map_err(|MoneyError::Overflow| StoredBalanceError::Balance(BalanceError::Overflow))
    }
}
