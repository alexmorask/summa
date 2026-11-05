//! Saving a transaction: all of it, or none of it.

use std::collections::BTreeSet;

use chrono::SubsecRound;
use sqlx::{Postgres, Transaction};

use super::error::{SaveError, violates};
use super::load::load_by_key;
use super::rows::{CorruptData, CurrencyCode, account_type_text, parse_account_type};
use super::{Saved, Store, StoredTag};
use crate::account::{AccountKind, AccountNumber, Side};
use crate::idempotency::same_contents;
use crate::money::Usd;
use crate::transaction::{PostError, PostedTransaction, TransactionId, UnpostedTransaction};

/// What one attempt to insert a transaction came to.
pub(crate) enum Attempt<T: StoredTag> {
    /// The transaction and all its entries and tags are written (committed
    /// once the caller commits).
    Inserted(PostedTransaction<Usd, T>),
    /// Another transaction already owns the idempotency key. Nothing was written.
    KeyTaken,
}

/// The distinct accounts `posted` has entries on, ascending. Inserting in
/// this order means two concurrent saves wait on each other's rows instead of
/// deadlocking. It also lists the accounts to mark confirmed after commit.
pub(crate) fn accounts_to_record<T: StoredTag>(
    posted: &PostedTransaction<Usd, T>,
) -> Vec<AccountNumber> {
    posted
        .entries()
        .iter()
        .map(|entry| entry.account())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// 4713-01-01 BC: PostgreSQL's lower timestamptz bound, as a year-numbered date.
fn earliest_storable() -> chrono::DateTime<chrono::Utc> {
    chrono::NaiveDate::from_ymd_opt(-4712, 1, 1)
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|naive| naive.and_utc())
        .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC)
}

impl<T: StoredTag> Store<T> {
    /// Records `transaction` in the ledger, stamping it with the next ID and
    /// the clock's time. Everything is written in one database transaction, so
    /// a failure leaves nothing behind.
    ///
    /// Saving the same request again (same key, same contents) records
    /// nothing new and returns [`Saved::AlreadyRecorded`] with the original.
    /// Reusing a key for different contents is [`SaveError::Conflict`].
    /// Reversing a transaction that already has a reversal is
    /// [`SaveError::AlreadyReversed`]. A chart that changes a used account's
    /// type or contra flag is [`SaveError::AccountChanged`].
    ///
    /// IDs come from a database sequence, so a failed or conflicting save can
    /// leave a gap in the IDs. That is expected.
    pub async fn save(
        &self,
        transaction: &UnpostedTransaction<Usd, T>,
    ) -> Result<Saved<T>, SaveError> {
        // 0. Refuse input the database can never store, so the error isn't
        //    mistaken for a retryable outage.
        if transaction.description().contains('\0') {
            return Err(SaveError::Unstorable {
                what: "description contains a NUL byte",
            });
        }
        if transaction
            .entries()
            .iter()
            .flat_map(|entry| entry.tags())
            .any(|tag| tag.value().contains('\0'))
        {
            return Err(SaveError::Unstorable {
                what: "tag value contains a NUL byte",
            });
        }
        // PostgreSQL's timestamptz starts at 4713 BC (chrono year -4712). Its
        // upper bound is far beyond chrono's, so only the lower one can bite.
        let effective_at = transaction.effective_at();
        if effective_at < earliest_storable() {
            return Err(SaveError::Unstorable {
                what: "effective time is before 4713 BC",
            });
        }

        // 1. The database keeps microseconds; refuse an effective time it would change.
        if !effective_at.timestamp_subsec_nanos().is_multiple_of(1_000) {
            return Err(SaveError::TooPrecise { effective_at });
        }

        // 2. Read the clock once per save, truncated to what the database keeps.
        let recorded_at = self.clock.now().trunc_subsecs(6);

        // 3. A reversal must mirror an original that is in the ledger. This
        //    runs outside the database transaction: an original never changes.
        if let Some(original_id) = transaction.reverses() {
            // `load` answers None for an ID the database can't hold too.
            let not_found = SaveError::OriginalNotFound {
                original: original_id,
            };
            let original = self.load(original_id).await?.ok_or(not_found)?;
            transaction
                .check_reversal_of(&original)
                .map_err(SaveError::Reversal)?;
        }

        // 4-6. Defensive, and should be unreachable: a committed row can't
        //      vanish, so after `KeyTaken` the lookup shouldn't come back
        //      empty. A key whose owner rolled back between our insert and our
        //      look is the one case; the bounded retry is belt and braces.
        for _ in 0..2 {
            match self.insert(transaction, recorded_at).await? {
                Attempt::Inserted(posted) => return Ok(Saved::Recorded(posted)),
                Attempt::KeyTaken => {
                    let existing =
                        load_by_key::<T>(&self.pool, transaction.idempotency_key()).await?;
                    if let Some(existing) = existing {
                        return if same_contents(transaction, &existing) {
                            Ok(Saved::AlreadyRecorded(existing))
                        } else {
                            Err(SaveError::Conflict {
                                key: transaction.idempotency_key().clone(),
                                existing: existing.id(),
                            })
                        };
                    }
                }
            }
        }
        Err(SaveError::Unavailable(sqlx::Error::RowNotFound))
    }

    /// One attempt: a database transaction around [`Self::insert_on`].
    async fn insert(
        &self,
        transaction: &UnpostedTransaction<Usd, T>,
        recorded_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Attempt<T>, SaveError> {
        let mut tx = self.pool.begin().await.map_err(SaveError::from_database)?;
        let attempt = self.insert_on(&mut tx, transaction, recorded_at).await?;
        match attempt {
            Attempt::Inserted(ref posted) => {
                tx.commit().await.map_err(SaveError::from_database)?;
                // Only now are the accounts surely recorded.
                self.confirmed.mark_committed(&accounts_to_record(posted));
            }
            // Release the connection before the caller looks the key up.
            Attempt::KeyTaken => tx.rollback().await.map_err(SaveError::from_database)?,
        }
        Ok(attempt)
    }

    /// Takes an ID, posts, and writes the transaction row, then the entries,
    /// then the tags, inside the database transaction `tx`. The caller
    /// commits it on `Inserted` and rolls it back on `KeyTaken` (or on
    /// error). Taking a `Transaction` rather than a bare connection means the
    /// compiler refuses to run these inserts outside a database transaction.
    /// It is a separate method so a test can hold the transaction open and
    /// force a concurrent save to wait on it.
    pub(crate) async fn insert_on(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        transaction: &UnpostedTransaction<Usd, T>,
        recorded_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Attempt<T>, SaveError> {
        let raw_id = sqlx::query_scalar!(r#"SELECT nextval('transaction_ids') AS "id!""#)
            .fetch_one(&mut **tx)
            .await
            .map_err(SaveError::from_database)?;
        let id = u64::try_from(raw_id).map_err(|_| {
            SaveError::Corrupt(CorruptData::OutOfRange {
                transaction: raw_id,
                column: "id",
                value: raw_id,
            })
        })?;

        let posted = transaction
            .clone()
            .post(&self.chart, TransactionId::new(id), recorded_at)
            .map_err(SaveError::Rejected)?;

        // Step 3 already found the original, so its ID fits.
        let reverses = posted
            .reverses()
            .map(|original| {
                i64::try_from(original.get()).map_err(|_| SaveError::OriginalNotFound { original })
            })
            .transpose()?;

        let inserted = sqlx::query_scalar!(
            "INSERT INTO transactions (id, idempotency_key, description, effective_at, recorded_at, reverses) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (idempotency_key) DO NOTHING \
             RETURNING id",
            raw_id,
            posted.idempotency_key().as_str(),
            posted.description(),
            posted.effective_at(),
            posted.recorded_at(),
            reverses
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| match posted.reverses() {
            Some(original) if violates(&e, "transactions_reverses_unique") => {
                SaveError::AlreadyReversed { original }
            }
            _ => SaveError::from_database(e),
        })?;
        if inserted.is_none() {
            return Ok(Attempt::KeyTaken);
        }

        // After the key insert on purpose: a retry of an already-recorded
        // request returned above, so it is never checked against today's
        // chart. Only new facts are checked.
        self.record_accounts(tx, &posted).await?;

        for (index, entry) in posted.entries().iter().enumerate() {
            let position = i16::try_from(index).map_err(|_| {
                SaveError::Corrupt(CorruptData::OutOfRange {
                    transaction: raw_id,
                    column: "position",
                    value: i64::try_from(index).unwrap_or(i64::MAX),
                })
            })?;
            let side = match entry.side() {
                Side::Debit => "debit",
                Side::Credit => "credit",
            };
            sqlx::query!(
                "INSERT INTO entries (transaction_id, position, account, side, amount, currency) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
                raw_id,
                position,
                i64::from(entry.account().get()),
                side,
                entry.amount().minor_units(),
                CurrencyCode::Usd.as_str()
            )
            .execute(&mut **tx)
            .await
            .map_err(SaveError::from_database)?;

            for tag in entry.tags() {
                sqlx::query!(
                    "INSERT INTO entry_tags (transaction_id, position, kind, value) \
                     VALUES ($1, $2, $3, $4)",
                    raw_id,
                    position,
                    tag.kind(),
                    tag.value()
                )
                .execute(&mut **tx)
                .await
                .map_err(SaveError::from_database)?;
            }
        }

        Ok(Attempt::Inserted(posted))
    }

    /// Records the type and contra flag of each account `posted` uses that
    /// this store hasn't confirmed, and checks them against the chart. The
    /// first entry on an account fixes both for good.
    async fn record_accounts(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        posted: &PostedTransaction<Usd, T>,
    ) -> Result<(), SaveError> {
        let pending = self.confirmed.unconfirmed(&accounts_to_record(posted));
        if pending.is_empty() {
            return Ok(());
        }

        // Ascending order (see `accounts_to_record`); the UNNEST insert below
        // preserves array order, so concurrent saves lock rows in the same order.
        let mut accounts = Vec::with_capacity(pending.len());
        let mut numbers = Vec::with_capacity(pending.len());
        let mut types = Vec::with_capacity(pending.len());
        let mut contras = Vec::with_capacity(pending.len());
        for number in &pending {
            // `post` has already confirmed every account is in the chart, so
            // this is unreachable; it is still a rejection, never a retry.
            let account = self.chart.get(*number).ok_or_else(|| {
                let entry = posted
                    .entries()
                    .iter()
                    .position(|entry| entry.account() == *number)
                    .unwrap_or(0);
                SaveError::Rejected(PostError::UnknownAccount {
                    entry,
                    account: *number,
                })
            })?;
            accounts.push(account);
            numbers.push(i64::from(number.get()));
            types.push(account_type_text(account.account_type()));
            contras.push(account.is_contra());
        }

        sqlx::query!(
            "INSERT INTO recorded_accounts (number, account_type, contra) \
             SELECT * FROM UNNEST($1::bigint[], $2::text[], $3::bool[]) \
             ON CONFLICT (number) DO NOTHING",
            &numbers,
            &types as &[&str],
            &contras
        )
        .execute(&mut **tx)
        .await
        .map_err(SaveError::from_database)?;

        // A separate statement: under Read Committed a statement only sees rows
        // committed before it started, so only a new SELECT sees a concurrent
        // first use that committed while the INSERT waited.
        let rows = sqlx::query!(
            "SELECT number, account_type, contra FROM recorded_accounts WHERE number = ANY($1)",
            &numbers
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(SaveError::from_database)?;

        for (number, account) in pending.iter().zip(&accounts) {
            let raw = i64::from(number.get());
            let row = rows
                .iter()
                .find(|row| row.number == raw)
                .ok_or(SaveError::Corrupt(CorruptData::MissingRecordedAccount {
                    account: raw,
                }))?;
            let recorded_type = parse_account_type(&row.account_type).ok_or_else(|| {
                SaveError::Corrupt(CorruptData::UnknownAccountType {
                    account: raw,
                    value: row.account_type.clone(),
                })
            })?;
            let recorded = AccountKind::new(recorded_type, row.contra);
            if recorded != account.kind() {
                return Err(SaveError::AccountChanged {
                    account: *number,
                    recorded,
                    chart: account.kind(),
                });
            }
        }
        Ok(())
    }
}
