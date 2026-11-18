//! The balance worker: reads the next ledger transactions after the
//! checkpoint, nets them in memory, and writes the new totals and the moved
//! checkpoint in one database transaction, so a batch counts once or not at
//! all.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::num::NonZeroU32;
use std::time::Duration;

use sqlx::{PgPool, Postgres, Transaction};

use super::error::{InvalidWorkerConfig, WorkerError};
use super::netting::{
    BalanceKey, LogEntry, LogTransaction, NettingOverflow, Totals, first_unbalanced, keys_for, net,
};
use super::rows::{CorruptData, EntryRow, ParsedEntry, TagRow, parse_entry};
use crate::transaction::TransactionId;

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
    /// The longest any retry wait grows to. It must be at least `retry_wait`.
    pub max_retry_wait: Duration,
}

impl Default for WorkerConfig {
    /// Batches of 1,000, a 1 s idle wait, and 5 retries starting at 100 ms and
    /// never waiting longer than 30 s.
    fn default() -> Self {
        Self {
            batch_size: const { NonZeroU32::new(1_000).unwrap() },
            idle_wait: Duration::from_secs(1),
            retries: 5,
            retry_wait: Duration::from_millis(100),
            max_retry_wait: Duration::from_secs(30),
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
    /// A worker over `pool`, batching and waiting as `config` says. It
    /// refuses a config whose first retry wait is longer than the cap.
    pub fn new(pool: PgPool, config: WorkerConfig) -> Result<Self, InvalidWorkerConfig> {
        if config.retry_wait > config.max_retry_wait {
            return Err(InvalidWorkerConfig::RetryWaitAboveCap {
                retry_wait: config.retry_wait,
                max_retry_wait: config.max_retry_wait,
            });
        }
        Ok(Self { pool, config })
    }

    /// Runs one batch in its own database transaction. On error nothing is
    /// written.
    pub async fn run_batch(&self) -> Result<BatchReport, WorkerError> {
        let mut tx = self.pool.begin().await.map_err(unbatched)?;
        let report = self.batch_on(&mut tx).await?;
        tx.commit().await.map_err(unbatched)?;
        Ok(report)
    }

    /// Runs batches until one finds nothing below the safe line, so it counts
    /// the log up to the safe line, not the whole log. Committed postings the
    /// line is still holding back are left for a later call, and
    /// [`lag`](Self::lag) counts them. Returns how many ledger transactions
    /// it counted.
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

    /// Runs batches until `should_stop` says so. A full batch goes again at
    /// once; a smaller one waits `idle_wait`. A retryable error waits
    /// `retry_wait`, doubling each time up to `max_retry_wait`, and retries up
    /// to `retries` failures in a row (any success resets the count). A
    /// permanent error, or running out of retries, returns the error. All
    /// waiting goes through `pause`. `should_stop` is checked before each
    /// batch, not during a `pause`, so stopping can take up to one wait. If it
    /// says stop while the latest batch had failed (a retry wait was
    /// interrupted), `run` returns that failure instead of `Ok`, so a
    /// shutdown never hides that the worker was not working.
    pub async fn run(
        &self,
        should_stop: impl Fn() -> bool,
        pause: impl AsyncFn(Duration),
    ) -> Result<(), WorkerError> {
        let mut failures = 0_u32;
        let mut latest_failure = None;
        while !should_stop() {
            match self.run_batch().await {
                Ok(report) => {
                    failures = 0;
                    latest_failure = None;
                    if report.transactions < self.config.batch_size.get() {
                        pause(self.config.idle_wait).await;
                    }
                }
                Err(error) if error.is_retryable() && failures < self.config.retries => {
                    let wait = self
                        .config
                        .retry_wait
                        .saturating_mul(2_u32.saturating_pow(failures))
                        .min(self.config.max_retry_wait);
                    failures += 1;
                    latest_failure = Some(error);
                    pause(wait).await;
                }
                Err(error) => return Err(error),
            }
        }
        latest_failure.map_or(Ok(()), Err)
    }

    /// Clears both balance tables and returns the checkpoint to the start, in
    /// one database transaction. It takes the checkpoint lock first, like a
    /// batch, so it waits for a running batch and no batch sees a half-reset.
    /// The reset commits an empty state, and a missing row reads as zero, so
    /// until the log is replayed (see [`rebuild`](Self::rebuild)) stored
    /// totals start from zero and net balances can read above or below the
    /// log's.
    pub async fn reset(&self) -> Result<(), WorkerError> {
        let mut tx = self.pool.begin().await.map_err(unbatched)?;
        self.reset_on(&mut tx).await?;
        tx.commit().await.map_err(unbatched)
    }

    /// The reset inside `tx`: everything but begin and commit. A separate
    /// method so a test can hold the reset open.
    pub(crate) async fn reset_on(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<(), WorkerError> {
        lock_checkpoint(tx).await?;
        sqlx::query!("DELETE FROM tag_balances")
            .execute(&mut **tx)
            .await
            .map_err(unbatched)?;
        sqlx::query!("DELETE FROM account_balances")
            .execute(&mut **tx)
            .await
            .map_err(unbatched)?;
        write_checkpoint(tx, &Checkpoint::START)
            .await
            .map_err(unbatched)
    }

    /// Resets, then replays the log up to the safe line, as
    /// [`catch_up`](Self::catch_up) does; postings the line is still holding
    /// back are left for later and counted by [`lag`](Self::lag). Returns how
    /// many ledger transactions it counted. The reset commits before the
    /// replay, so during a rebuild stored balances are correct as of the
    /// checkpoint: totals start from zero, and a net balance can read above or
    /// below its final value mid-replay. Reads are unreliable until the
    /// rebuild finishes.
    pub async fn rebuild(&self) -> Result<u64, WorkerError> {
        self.reset().await?;
        self.catch_up().await
    }

    /// How many committed ledger transactions are after the checkpoint,
    /// whether or not the safe line lets the next batch count them yet. A
    /// posting held back by a long-running transaction elsewhere is lag. One
    /// statement reads the checkpoint and the log together, but the answer is
    /// only a point-in-time measure: postings keep arriving. A missing
    /// checkpoint row is an error (`Corrupt`), never zero lag.
    pub async fn lag(&self) -> Result<u64, WorkerError> {
        let row = sqlx::query!(
            r#"SELECT (SELECT count(*) FROM transactions t
                       WHERE (t.db_tx_id, t.id) > (c.db_tx_id, c.transaction_id)) AS "lag!"
               FROM balance_checkpoint c"#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(unbatched)?
        .ok_or(WorkerError::Corrupt(CorruptData::MissingCheckpoint))?;
        // A count is never negative.
        Ok(row.lag.cast_unsigned())
    }

    /// One batch inside `tx`: everything but begin and commit. A separate
    /// method so a test can hold the batch open or abandon it.
    pub(crate) async fn batch_on(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<BatchReport, WorkerError> {
        // First, and locked: a second worker waits here until this batch
        // commits, then reads the checkpoint this one moved.
        let checkpoint = lock_checkpoint(tx).await?;

        // Only below the safe line: every database transaction older than the
        // snapshot's xmin has finished, so nothing can still commit behind
        // the checkpoint. sqlx has no `xid8` type, so it travels as text
        // (`::text::bigint` to read, `$n::text::xid8` to bind); the values stay
        // far below `i64::MAX`. The seed `'0'::xid8` means "nothing counted".
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
        .map_err(unbatched)?;
        let Some(last) = next.last() else {
            return Ok(BatchReport { transactions: 0 });
        };
        let end = Checkpoint {
            db_tx_id: last.db_tx_id,
            transaction_id: last.id,
        };
        let ids: Vec<i64> = next.iter().map(|row| row.id).collect();
        // From here on a database error names the batch.
        let batch = (transaction_id(ids[0]), transaction_id(end.transaction_id));
        let batched = |error| WorkerError::from_database(error, Some(batch));

        let transactions = load_log(tx, &ids, batch).await?;
        // The save path only writes balanced transactions; one that isn't was
        // written around it, and adding it would unbalance the stored totals.
        if let Some(transaction) = first_unbalanced(&transactions) {
            return Err(WorkerError::Corrupt(CorruptData::Unbalanced {
                transaction,
            }));
        }

        let stored = stored_totals(tx, &transactions, batch).await?;
        let existing: HashSet<BalanceKey> = stored.iter().map(|(key, _)| key.clone()).collect();
        let updates = net(stored, &transactions).map_err(|NettingOverflow { transaction }| {
            WorkerError::Overflow {
                transaction: transaction_id(transaction),
            }
        })?;
        write_totals(tx, updates, &existing)
            .await
            .map_err(batched)?;

        write_checkpoint(tx, &end).await.map_err(batched)?;

        Ok(BatchReport {
            transactions: u32::try_from(ids.len())
                .expect("a batch never holds more transactions than its u32 limit"),
        })
    }
}

/// A ledger transaction id as stored, for naming it in an error.
fn transaction_id(id: i64) -> TransactionId {
    // Ids come from an identity column and start at 1.
    TransactionId::new(id.cast_unsigned())
}

/// Classifies a database error that happened before a batch was read.
fn unbatched(error: sqlx::Error) -> WorkerError {
    WorkerError::from_database(error, None)
}

/// Locks the checkpoint row and returns it. Every writer of the balance tables
/// takes this lock first, so only one runs at a time and statement order may
/// differ between writers without risking a deadlock. Tests that write balance
/// rows directly bypass the lock on purpose.
async fn lock_checkpoint(tx: &mut Transaction<'_, Postgres>) -> Result<Checkpoint, WorkerError> {
    let row = sqlx::query!(
        r#"SELECT db_tx_id::text::bigint AS "db_tx_id!", transaction_id
           FROM balance_checkpoint FOR UPDATE"#
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(unbatched)?
    .ok_or(WorkerError::Corrupt(CorruptData::MissingCheckpoint))?;
    Ok(Checkpoint {
        db_tx_id: row.db_tx_id,
        transaction_id: row.transaction_id,
    })
}

/// Where the worker has counted up to.
struct Checkpoint {
    db_tx_id: i64,
    transaction_id: i64,
}

impl Checkpoint {
    /// Before the first ledger transaction: nothing counted yet.
    const START: Self = Self {
        db_tx_id: 0,
        transaction_id: 0,
    };
}

/// Moves the checkpoint to `checkpoint`. The caller holds the checkpoint lock.
async fn write_checkpoint(
    tx: &mut Transaction<'_, Postgres>,
    checkpoint: &Checkpoint,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE balance_checkpoint SET db_tx_id = $1::text::xid8, transaction_id = $2",
        checkpoint.db_tx_id.to_string(),
        checkpoint.transaction_id
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The stored totals of every key `transactions` add to. A key with no
/// row is left out, so netting starts it at zero. `batch` is named by a
/// database error.
async fn stored_totals(
    tx: &mut Transaction<'_, Postgres>,
    transactions: &[LogTransaction],
    batch: (TransactionId, TransactionId),
) -> Result<Vec<(BalanceKey, Totals)>, WorkerError> {
    let batched = |error| WorkerError::from_database(error, Some(batch));
    let keys = keys_for(transactions);
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
    .map_err(batched)?;
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
    .map_err(batched)?;

    let negative = |account| WorkerError::Corrupt(CorruptData::NegativeTotal { account });
    let mut stored_accounts: HashMap<i64, Totals> = HashMap::new();
    for row in account_rows {
        let totals = Totals::from_stored(row.debit_total, row.credit_total)
            .ok_or_else(|| negative(row.account))?;
        stored_accounts.insert(row.account, totals);
    }
    let mut stored_tags: HashMap<(i64, String, String), Totals> = HashMap::new();
    for row in tag_rows {
        let totals = Totals::from_stored(row.debit_total, row.credit_total)
            .ok_or_else(|| negative(row.account))?;
        stored_tags.insert((row.account, row.kind, row.value), totals);
    }

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
/// `batch` is the first and last id, named by a database error.
async fn load_log(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[i64],
    batch: (TransactionId, TransactionId),
) -> Result<Vec<LogTransaction>, WorkerError> {
    let batched = |error| WorkerError::from_database(error, Some(batch));
    let entries = sqlx::query_as!(
        EntryRow,
        "SELECT transaction_id, position, account, side, amount, currency \
         FROM entries WHERE transaction_id = ANY($1) ORDER BY transaction_id, position",
        ids
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(batched)?;
    let tags = sqlx::query_as!(
        TagRow,
        "SELECT transaction_id, position, kind, value \
         FROM entry_tags WHERE transaction_id = ANY($1) ORDER BY transaction_id, position, kind",
        ids
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(batched)?;

    let mut parsed: HashMap<i64, ParsedEntries> = HashMap::new();
    for row in entries {
        let transaction = row.transaction_id;
        let ParsedEntry {
            position,
            account,
            side,
            amount,
        } = parse_entry(transaction, row).map_err(WorkerError::Corrupt)?;
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

/// The columns of the account totals to write, one entry per account.
#[derive(Default)]
struct AccountColumns {
    accounts: Vec<i64>,
    debits: Vec<i64>,
    credits: Vec<i64>,
}

/// The columns of the tag totals to write, one entry per account and tag.
#[derive(Default)]
struct TagColumns {
    accounts: Vec<i64>,
    kinds: Vec<String>,
    values: Vec<String>,
    debits: Vec<i64>,
    credits: Vec<i64>,
}

/// Writes `updates` as absolute totals. A key in `existing` must already have
/// a row and is updated; any other key must not, and is inserted. Either
/// assumption failing means the stored rows changed under the checkpoint lock,
/// so the batch fails instead of overwriting or duplicating history: a
/// duplicate insert is a unique violation, and an update that finds fewer rows
/// than it expected is `RowNotFound`.
pub(super) async fn write_totals(
    tx: &mut Transaction<'_, Postgres>,
    updates: Vec<(BalanceKey, Totals)>,
    existing: &HashSet<BalanceKey>,
) -> Result<(), sqlx::Error> {
    let (mut new_accounts, mut old_accounts) =
        (AccountColumns::default(), AccountColumns::default());
    let (mut new_tags, mut old_tags) = (TagColumns::default(), TagColumns::default());
    for (key, totals) in updates {
        let old = existing.contains(&key);
        let (debit, credit) = (totals.debit.minor_units(), totals.credit.minor_units());
        match key {
            BalanceKey::Account(account) => {
                let columns = if old {
                    &mut old_accounts
                } else {
                    &mut new_accounts
                };
                columns.accounts.push(i64::from(account.get()));
                columns.debits.push(debit);
                columns.credits.push(credit);
            }
            BalanceKey::Tag(account, kind, value) => {
                let columns = if old { &mut old_tags } else { &mut new_tags };
                columns.accounts.push(i64::from(account.get()));
                columns.kinds.push(kind);
                columns.values.push(value);
                columns.debits.push(debit);
                columns.credits.push(credit);
            }
        }
    }

    if !new_accounts.accounts.is_empty() {
        sqlx::query!(
            "INSERT INTO account_balances (account, debit_total, credit_total) \
             SELECT * FROM UNNEST($1::bigint[], $2::bigint[], $3::bigint[])",
            &new_accounts.accounts,
            &new_accounts.debits,
            &new_accounts.credits
        )
        .execute(&mut **tx)
        .await?;
    }
    if !old_accounts.accounts.is_empty() {
        let done = sqlx::query!(
            "UPDATE account_balances b \
             SET debit_total = u.debit_total, credit_total = u.credit_total \
             FROM UNNEST($1::bigint[], $2::bigint[], $3::bigint[]) \
                  AS u(account, debit_total, credit_total) \
             WHERE b.account = u.account",
            &old_accounts.accounts,
            &old_accounts.debits,
            &old_accounts.credits
        )
        .execute(&mut **tx)
        .await?;
        expect_rows(done.rows_affected(), old_accounts.accounts.len())?;
    }
    if !new_tags.accounts.is_empty() {
        sqlx::query!(
            "INSERT INTO tag_balances (account, kind, value, debit_total, credit_total) \
             SELECT * FROM UNNEST($1::bigint[], $2::text[], $3::text[], $4::bigint[], $5::bigint[])",
            &new_tags.accounts,
            &new_tags.kinds,
            &new_tags.values,
            &new_tags.debits,
            &new_tags.credits
        )
        .execute(&mut **tx)
        .await?;
    }
    if !old_tags.accounts.is_empty() {
        let done = sqlx::query!(
            "UPDATE tag_balances b \
             SET debit_total = u.debit_total, credit_total = u.credit_total \
             FROM UNNEST($1::bigint[], $2::text[], $3::text[], $4::bigint[], $5::bigint[]) \
                  AS u(account, kind, value, debit_total, credit_total) \
             WHERE b.account = u.account AND b.kind = u.kind AND b.value = u.value",
            &old_tags.accounts,
            &old_tags.kinds,
            &old_tags.values,
            &old_tags.debits,
            &old_tags.credits
        )
        .execute(&mut **tx)
        .await?;
        expect_rows(done.rows_affected(), old_tags.accounts.len())?;
    }
    Ok(())
}

/// `RowNotFound` unless an update touched exactly the rows it should have.
fn expect_rows(touched: u64, expected: usize) -> Result<(), sqlx::Error> {
    if u64::try_from(expected).is_ok_and(|expected| touched == expected) {
        Ok(())
    } else {
        Err(sqlx::Error::RowNotFound)
    }
}
