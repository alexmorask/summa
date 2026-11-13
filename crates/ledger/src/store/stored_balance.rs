//! Reading a balance from the totals the balance worker keeps.

use super::error::StoredBalanceError;
use super::netting::Totals;
use super::rows::CorruptData;
use super::{Store, StoredTag};
use crate::account::AccountNumber;
use crate::balance::BalanceError;
use crate::money::{Money, MoneyError, Usd};
use crate::transaction::TransactionId;

/// A stored balance and how far the worker had counted when it was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredBalance {
    /// The net balance on the account's normal side.
    pub balance: Money<Usd>,
    /// The last ledger transaction the worker had counted, in the order it
    /// reads the log, when the balance was read. `None` means nothing is
    /// counted yet: a new ledger, or a rebuild that has just reset. The
    /// balance and this value come from one query, so they always agree.
    pub counted_through: Option<TransactionId>,
}

impl<T: StoredTag> Store<T> {
    /// The stored balance of `account`, or of `account` restricted to `tag`,
    /// with the checkpoint it was read at. It is the same number `balance`
    /// gives over every transaction counted up to that checkpoint, with no
    /// cutoffs. An account or tag with no stored row reads as zero. An account
    /// missing from the chart is `Balance(UnknownAccount)`.
    ///
    /// It can lag behind the log (see `BalanceWorker::lag`), so never use it
    /// to decide whether a spend is allowed. A stopped or held-back worker
    /// leaves values stale, and during a rebuild a net balance can read above
    /// or below its final value mid-replay. `counted_through` is how a caller
    /// tells: a balance is only final once it has reached the newest
    /// transaction and no rebuild is running.
    pub async fn stored_balance(
        &self,
        account: AccountNumber,
        tag: Option<&T>,
    ) -> Result<StoredBalance, StoredBalanceError> {
        let normal_side = self
            .chart
            .get(account)
            .ok_or(StoredBalanceError::Balance(BalanceError::UnknownAccount {
                account,
            }))?
            .normal_side();
        let number = i64::from(account.get());
        // One statement, so the totals and the checkpoint are from the same
        // moment. The checkpoint always has a row; the balance row may not.
        let (counted, debit, credit) = match tag {
            None => sqlx::query!(
                r#"SELECT c.transaction_id,
                          b.debit_total AS "debit_total?", b.credit_total AS "credit_total?"
                   FROM balance_checkpoint c
                   LEFT JOIN account_balances b ON b.account = $1"#,
                number
            )
            .fetch_optional(&self.pool)
            .await
            .map_err(StoredBalanceError::Unavailable)?
            .map(|row| (row.transaction_id, row.debit_total, row.credit_total)),
            Some(tag) => sqlx::query!(
                r#"SELECT c.transaction_id,
                          b.debit_total AS "debit_total?", b.credit_total AS "credit_total?"
                   FROM balance_checkpoint c
                   LEFT JOIN tag_balances b
                     ON b.account = $1 AND b.kind = $2 AND b.value = $3"#,
                number,
                tag.kind(),
                tag.value()
            )
            .fetch_optional(&self.pool)
            .await
            .map_err(StoredBalanceError::Unavailable)?
            .map(|row| (row.transaction_id, row.debit_total, row.credit_total)),
        }
        .ok_or(StoredBalanceError::Corrupt(CorruptData::MissingCheckpoint))?;
        let totals = Totals::from_stored(debit.unwrap_or(0), credit.unwrap_or(0)).ok_or(
            StoredBalanceError::Corrupt(CorruptData::NegativeTotal { account: number }),
        )?;
        let balance = totals
            .net(normal_side)
            .map_err(|MoneyError::Overflow| StoredBalanceError::Balance(BalanceError::Overflow))?;
        Ok(StoredBalance {
            balance,
            // The seed checkpoint is id 0, and real ids start at 1.
            counted_through: (counted > 0).then(|| TransactionId::new(counted.cast_unsigned())),
        })
    }
}
