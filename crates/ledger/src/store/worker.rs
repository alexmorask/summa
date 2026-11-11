//! The balance worker: reads the next ledger transactions after the
//! checkpoint, nets them in memory, and writes the new totals and the moved
//! checkpoint in one database transaction, so a batch counts once or not at
//! all.

use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroU32;
use std::time::Duration;

use sqlx::{PgPool, Postgres, Transaction};

use super::error::WorkerError;
use super::netting::{BalanceKey, LogEntry, LogTransaction, Netting, NettingOverflow, Totals};
use super::rows::{CorruptData, EntryRow, TagRow, parse_entry};
use crate::money::Money;

/// How the worker batches, waits and retries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerConfig {
    /// The most ledger transactions one batch reads.
    pub batch_size: NonZeroU32,
    /// How long to wait after a batch that wasn't full.
    pub idle_wait: Duration,
    /// How many failures in a row to retry before giving up.
    pub retries: u32,
    /// The wait before the first retry.
    pub retry_wait: Duration,
}

impl Default for WorkerConfig {
    /// Batches of 1,000, a 1 s idle wait, and 5 retries starting at 100 ms.
    fn default() -> Self {
        Self {
            batch_size: const { NonZeroU32::new(1_000).unwrap() },
            idle_wait: Duration::from_secs(1),
            retries: 5,
            retry_wait: Duration::from_millis(100),
        }
    }
}

/// What one batch did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchReport {
    /// How many ledger transactions it counted.
    pub transactions: u32,
}

/// Keeps the stored account and tag balances up to date from the entry log.
pub struct BalanceWorker {
    pool: PgPool,
    config: WorkerConfig,
}

/// A ledger transaction's entries by position, as they are parsed.
type ParsedEntries = BTreeMap<i16, LogEntry>;

impl BalanceWorker {
    /// A worker over `pool`, batching and waiting as `config` says.
    pub fn new(pool: PgPool, config: WorkerConfig) -> Self {
        Self { pool, config }
    }

    /// Runs one batch in its own database transaction. On error nothing is
    /// written.
    pub async fn run_batch(&self) -> Result<BatchReport, WorkerError> {
        let mut tx = self.pool.begin().await.map_err(WorkerError::Unavailable)?;
        let report = self.batch_on(&mut tx).await?;
        tx.commit().await.map_err(WorkerError::Unavailable)?;
        Ok(report)
    }

    /// Runs batches until one finds nothing new. Returns how many ledger
    /// transactions they counted.
    pub async fn catch_up(&self) -> Result<u64, WorkerError> {
        let mut total = 0_u64;
        loop {
            let report = self.run_batch().await?;
            if report.transactions == 0 {
                return Ok(total);
            }
            total += u64::from(report.transactions);
        }
    }

    /// One batch inside `tx`: everything but begin and commit. A separate
    /// method so a test can hold the batch open or abandon it.
    pub(crate) async fn batch_on(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<BatchReport, WorkerError> {
        let checkpoint = sqlx::query!(
            r#"SELECT db_tx_id::text::bigint AS "db_tx_id!", transaction_id FROM balance_checkpoint"#
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(WorkerError::Unavailable)?;

        // Only below the safe line: every database transaction older than the
        // snapshot's xmin has finished, so nothing can still commit behind
        // the checkpoint.
        let next = sqlx::query!(
            r#"SELECT id, db_tx_id::text::bigint AS "db_tx_id!" FROM transactions
               WHERE (db_tx_id, id) > ($1::text::xid8, $2)
                 AND db_tx_id < pg_snapshot_xmin(pg_current_snapshot())
               ORDER BY db_tx_id, id
               LIMIT $3"#,
            checkpoint.db_tx_id.to_string(),
            checkpoint.transaction_id,
            i64::from(self.config.batch_size.get())
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(WorkerError::Unavailable)?;
        let Some(last) = next.last() else {
            return Ok(BatchReport { transactions: 0 });
        };
        let (last_db_tx_id, last_id) = (last.db_tx_id, last.id);
        let ids: Vec<i64> = next.iter().map(|row| row.id).collect();

        let transactions = load_log(tx, &ids).await?;

        let mut netting = Netting::new(stored_totals(tx, &transactions).await?);
        for transaction in &transactions {
            netting
                .apply(transaction)
                .map_err(|NettingOverflow { transaction }| WorkerError::Overflow { transaction })?;
        }
        write_totals(tx, netting.updates()).await?;

        sqlx::query!(
            "UPDATE balance_checkpoint SET db_tx_id = $1::text::xid8, transaction_id = $2",
            last_db_tx_id.to_string(),
            last_id
        )
        .execute(&mut **tx)
        .await
        .map_err(WorkerError::Unavailable)?;

        Ok(BatchReport {
            transactions: u32::try_from(ids.len()).unwrap_or(u32::MAX),
        })
    }
}

/// The stored totals of every key `transactions` add to. A key with no
/// row is left out, so netting starts it at zero.
async fn stored_totals(
    tx: &mut Transaction<'_, Postgres>,
    transactions: &[LogTransaction],
) -> Result<Vec<(BalanceKey, Totals)>, WorkerError> {
    let keys = Netting::keys_for(transactions);
    let mut accounts = Vec::new();
    let (mut tag_accounts, mut kinds, mut values) = (Vec::new(), Vec::new(), Vec::new());
    for key in &keys {
        match key {
            BalanceKey::Account(account) => accounts.push(i64::from(account.get())),
            BalanceKey::Tag(account, kind, value) => {
                tag_accounts.push(i64::from(account.get()));
                kinds.push(kind.clone());
                values.push(value.clone());
            }
        }
    }

    let account_rows = sqlx::query!(
        "SELECT account, debit_total, credit_total FROM account_balances \
         WHERE account = ANY($1)",
        &accounts
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(WorkerError::Unavailable)?;
    let tag_rows = sqlx::query!(
        "SELECT account, kind, value, debit_total, credit_total FROM tag_balances \
         WHERE (account, kind, value) IN \
               (SELECT * FROM UNNEST($1::bigint[], $2::text[], $3::text[]))",
        &tag_accounts,
        &kinds,
        &values
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(WorkerError::Unavailable)?;

    let totals = |debit, credit| Totals {
        debit: Money::from_minor_units(debit),
        credit: Money::from_minor_units(credit),
    };
    let stored_accounts: HashMap<i64, Totals> = account_rows
        .into_iter()
        .map(|row| (row.account, totals(row.debit_total, row.credit_total)))
        .collect();
    let stored_tags: HashMap<(i64, String, String), Totals> = tag_rows
        .into_iter()
        .map(|row| {
            (
                (row.account, row.kind, row.value),
                totals(row.debit_total, row.credit_total),
            )
        })
        .collect();

    // Matched on the numbers sent, so a stored account never needs parsing.
    Ok(keys
        .into_iter()
        .filter_map(|key| {
            let stored = match &key {
                BalanceKey::Account(account) => stored_accounts.get(&i64::from(account.get())),
                BalanceKey::Tag(account, kind, value) => {
                    stored_tags.get(&(i64::from(account.get()), kind.clone(), value.clone()))
                }
            };
            stored.copied().map(|totals| (key, totals))
        })
        .collect())
}

/// The entries and tags of the ledger transactions `ids`, in the order given.
async fn load_log(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[i64],
) -> Result<Vec<LogTransaction>, WorkerError> {
    let entries = sqlx::query_as!(
        EntryRow,
        "SELECT transaction_id, position, account, side, amount, currency \
         FROM entries WHERE transaction_id = ANY($1) ORDER BY transaction_id, position",
        ids
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(WorkerError::Unavailable)?;
    let tags = sqlx::query_as!(
        TagRow,
        "SELECT transaction_id, position, kind, value \
         FROM entry_tags WHERE transaction_id = ANY($1) ORDER BY transaction_id, position, kind",
        ids
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(WorkerError::Unavailable)?;

    let mut parsed: HashMap<i64, ParsedEntries> = HashMap::new();
    for row in entries {
        let transaction = row.transaction_id;
        let (position, account, side, amount) =
            parse_entry(transaction, row).map_err(WorkerError::Corrupt)?;
        let entry = LogEntry {
            account,
            side,
            amount,
            tags: Vec::new(),
        };
        if parsed
            .entry(transaction)
            .or_default()
            .insert(position, entry)
            .is_some()
        {
            return Err(WorkerError::Corrupt(CorruptData::DuplicatePosition {
                transaction,
                position,
            }));
        }
    }
    for row in tags {
        let Some(entry) = parsed
            .get_mut(&row.transaction_id)
            .and_then(|entries| entries.get_mut(&row.position))
        else {
            return Err(WorkerError::Corrupt(CorruptData::OutOfRange {
                transaction: row.transaction_id,
                column: "tag position",
                value: i64::from(row.position),
            }));
        };
        entry.tags.push((row.kind, row.value));
    }

    Ok(ids
        .iter()
        .map(|id| LogTransaction {
            id: *id,
            entries: parsed
                .remove(id)
                .unwrap_or_default()
                .into_values()
                .collect(),
        })
        .collect())
}

/// Writes `updates` as absolute totals, one statement per table.
async fn write_totals(
    tx: &mut Transaction<'_, Postgres>,
    updates: Vec<(BalanceKey, Totals)>,
) -> Result<(), WorkerError> {
    let (mut accounts, mut debits, mut credits) = (Vec::new(), Vec::new(), Vec::new());
    let (mut tag_accounts, mut kinds, mut values, mut tag_debits, mut tag_credits) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (key, totals) in updates {
        match key {
            BalanceKey::Account(account) => {
                accounts.push(i64::from(account.get()));
                debits.push(totals.debit.minor_units());
                credits.push(totals.credit.minor_units());
            }
            BalanceKey::Tag(account, kind, value) => {
                tag_accounts.push(i64::from(account.get()));
                kinds.push(kind);
                values.push(value);
                tag_debits.push(totals.debit.minor_units());
                tag_credits.push(totals.credit.minor_units());
            }
        }
    }

    if !accounts.is_empty() {
        sqlx::query!(
            "INSERT INTO account_balances (account, debit_total, credit_total) \
             SELECT * FROM UNNEST($1::bigint[], $2::bigint[], $3::bigint[]) \
             ON CONFLICT (account) DO UPDATE \
             SET debit_total = EXCLUDED.debit_total, credit_total = EXCLUDED.credit_total",
            &accounts,
            &debits,
            &credits
        )
        .execute(&mut **tx)
        .await
        .map_err(WorkerError::Unavailable)?;
    }
    if !tag_accounts.is_empty() {
        sqlx::query!(
            "INSERT INTO tag_balances (account, kind, value, debit_total, credit_total) \
             SELECT * FROM UNNEST($1::bigint[], $2::text[], $3::text[], $4::bigint[], $5::bigint[]) \
             ON CONFLICT (account, kind, value) DO UPDATE \
             SET debit_total = EXCLUDED.debit_total, credit_total = EXCLUDED.credit_total",
            &tag_accounts,
            &kinds,
            &values,
            &tag_debits,
            &tag_credits
        )
        .execute(&mut **tx)
        .await
        .map_err(WorkerError::Unavailable)?;
    }
    Ok(())
}
