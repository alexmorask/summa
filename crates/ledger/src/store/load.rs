//! Reading transactions back: three queries, then one assembly.

use sqlx::PgPool;

use super::error::LoadError;
use super::rows::{self, EntryRow, TagRow, TransactionRow};
use super::{Store, StoredTag};
use crate::idempotency::IdempotencyKey;
use crate::money::Usd;
use crate::transaction::{PostedTransaction, TransactionId};

impl<T: StoredTag> Store<T> {
    /// The transaction with this ID, or `None` if there isn't one.
    pub async fn load(
        &self,
        id: TransactionId,
    ) -> Result<Option<PostedTransaction<Usd, T>>, LoadError> {
        // An ID the database can't hold was never assigned.
        let Ok(id) = i64::try_from(id.get()) else {
            return Ok(None);
        };
        let row = sqlx::query_as!(
            TransactionRow,
            "SELECT id, idempotency_key, description, effective_at, recorded_at, reverses \
             FROM transactions WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(LoadError::Unavailable)?;
        match row {
            Some(row) => assemble_with_children(&self.pool, row).await.map(Some),
            None => Ok(None),
        }
    }
}

/// The transaction holding this key, or `None`. Used by `save` to look at the
/// transaction that already owns a key.
pub(super) async fn load_by_key<T: StoredTag>(
    pool: &PgPool,
    key: &IdempotencyKey,
) -> Result<Option<PostedTransaction<Usd, T>>, LoadError> {
    let row = sqlx::query_as!(
        TransactionRow,
        "SELECT id, idempotency_key, description, effective_at, recorded_at, reverses \
         FROM transactions WHERE idempotency_key = $1",
        key.as_str()
    )
    .fetch_optional(pool)
    .await
    .map_err(LoadError::Unavailable)?;
    match row {
        Some(row) => assemble_with_children(pool, row).await.map(Some),
        None => Ok(None),
    }
}

/// Fetches the entries and tags of `row` and assembles the whole transaction.
async fn assemble_with_children<T: StoredTag>(
    pool: &PgPool,
    row: TransactionRow,
) -> Result<PostedTransaction<Usd, T>, LoadError> {
    let entries = sqlx::query_as!(
        EntryRow,
        "SELECT transaction_id, position, account, side, amount, currency \
         FROM entries WHERE transaction_id = $1 ORDER BY position",
        row.id
    )
    .fetch_all(pool)
    .await
    .map_err(LoadError::Unavailable)?;
    let tags = sqlx::query_as!(
        TagRow,
        "SELECT transaction_id, position, kind, value \
         FROM entry_tags WHERE transaction_id = $1 ORDER BY position, kind",
        row.id
    )
    .fetch_all(pool)
    .await
    .map_err(LoadError::Unavailable)?;
    rows::assemble(row, entries, tags).map_err(LoadError::Corrupt)
}
