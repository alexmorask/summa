//! The tables refuse rows the ledger's rules forbid, whatever code writes them.
//! These tests use raw `sqlx::query` because they insert rows the typed code
//! would never produce.

use sqlx::{AssertSqlSafe, PgExecutor, PgPool};

/// Inserts a transaction row, with the given idempotency key, times and `reverses`.
async fn insert_transaction(
    executor: impl PgExecutor<'_>,
    id: i64,
    idempotency_key: &str,
    effective_at: &str,
    recorded_at: &str,
    reverses: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO transactions (id, idempotency_key, description, effective_at, recorded_at, reverses) \
         VALUES ($1, $2, 'test', $3::timestamptz, $4::timestamptz, $5)",
    )
    .bind(id)
    .bind(idempotency_key)
    .bind(effective_at)
    .bind(recorded_at)
    .bind(reverses)
    .execute(executor)
    .await
    .map(|_| ())
}

async fn insert_entry(
    executor: impl PgExecutor<'_>,
    transaction_id: i64,
    position: i16,
    account: i64,
    side: &str,
    amount: i64,
    currency: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO entries (transaction_id, position, account, side, amount, currency) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(transaction_id)
    .bind(position)
    .bind(account)
    .bind(side)
    .bind(amount)
    .bind(currency)
    .execute(executor)
    .await
    .map(|_| ())
}

/// A valid transaction with two balanced entries, committed.
async fn insert_valid(pool: &PgPool, id: i64, key: &str) {
    let mut tx = pool.begin().await.unwrap();
    insert_transaction(
        &mut *tx,
        id,
        key,
        "2026-01-01T00:00:00Z",
        "2026-01-02T00:00:00Z",
        None,
    )
    .await
    .unwrap();
    insert_entry(&mut *tx, id, 0, 1000, "debit", 500, "USD")
        .await
        .unwrap();
    insert_entry(&mut *tx, id, 1, 2000, "credit", 500, "USD")
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// Inserts a transaction row with fixed times, inside an open database transaction.
async fn begin_transaction_row(tx: &mut sqlx::PgConnection, id: i64, key: &str) {
    insert_transaction(
        tx,
        id,
        key,
        "2026-01-01T00:00:00Z",
        "2026-01-02T00:00:00Z",
        None,
    )
    .await
    .unwrap();
}

/// The message of the database error a failed statement or commit raised.
fn refusal(result: Result<(), sqlx::Error>) -> String {
    let error = result.expect_err("the database should have refused");
    error
        .as_database_error()
        .expect("the error should come from the database")
        .message()
        .to_owned()
}

/// Asserts that nothing was stored.
async fn assert_empty(pool: &PgPool) {
    for table in ["transactions", "entries"] {
        let count: i64 = sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "{table} should be empty");
    }
}

/// The name of the constraint a failed statement violated.
fn violated(result: Result<(), sqlx::Error>) -> String {
    let error = result.expect_err("the statement should have been rejected");
    error
        .as_database_error()
        .and_then(|e| e.constraint())
        .expect("the error should name a constraint")
        .to_owned()
}

#[sqlx::test]
async fn rejects_non_positive_amount(pool: PgPool) {
    insert_valid(&pool, 1, "valid").await;
    let mut tx = pool.begin().await.unwrap();
    begin_transaction_row(&mut tx, 2, "broken").await;
    insert_entry(&mut *tx, 2, 0, 1000, "debit", 500, "USD")
        .await
        .unwrap();
    let result = insert_entry(&mut *tx, 2, 1, 2000, "credit", 0, "USD").await;
    assert_eq!(violated(result), "entries_amount_positive");
}

#[sqlx::test]
async fn rejects_currency_other_than_usd(pool: PgPool) {
    insert_valid(&pool, 1, "valid").await;
    let result = insert_entry(&pool, 1, 2, 1000, "debit", 500, "EUR").await;
    assert_eq!(violated(result), "entries_currency_usd");
}

#[sqlx::test]
async fn rejects_unknown_side(pool: PgPool) {
    insert_valid(&pool, 1, "valid").await;
    let result = insert_entry(&pool, 1, 2, 1000, "left", 500, "USD").await;
    assert_eq!(violated(result), "entries_side_valid");
}

#[sqlx::test]
async fn rejects_effective_after_recorded(pool: PgPool) {
    let result = insert_transaction(
        &pool,
        1,
        "key",
        "2026-01-03T00:00:00Z",
        "2026-01-02T00:00:00Z",
        None,
    )
    .await;
    assert_eq!(
        violated(result),
        "transactions_effective_not_after_recorded"
    );
}

#[sqlx::test]
async fn rejects_duplicate_idempotency_key(pool: PgPool) {
    insert_valid(&pool, 1, "same-key").await;
    let result = insert_transaction(
        &pool,
        2,
        "same-key",
        "2026-01-01T00:00:00Z",
        "2026-01-02T00:00:00Z",
        None,
    )
    .await;
    assert_eq!(violated(result), "transactions_idempotency_key_unique");
}

#[sqlx::test]
async fn rejects_reverses_pointing_nowhere(pool: PgPool) {
    let result = insert_transaction(
        &pool,
        1,
        "key",
        "2026-01-01T00:00:00Z",
        "2026-01-02T00:00:00Z",
        Some(999),
    )
    .await;
    assert_eq!(violated(result), "transactions_reverses_fkey");
}

#[sqlx::test]
async fn rejects_two_tags_of_one_kind_on_one_entry(pool: PgPool) {
    insert_valid(&pool, 1, "valid").await;
    let insert_tag = |value: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO entry_tags (transaction_id, position, kind, value) \
                 VALUES (1, 0, 'customer', $1)",
            )
            .bind(value)
            .execute(&pool)
            .await
            .map(|_| ())
        }
    };
    insert_tag("a").await.unwrap();
    assert_eq!(violated(insert_tag("b").await), "entry_tags_pkey");
}

#[sqlx::test]
async fn accepts_account_number_u32_max(pool: PgPool) {
    let mut tx = pool.begin().await.unwrap();
    begin_transaction_row(&mut tx, 1, "key").await;
    insert_entry(&mut *tx, 1, 0, 4_294_967_295, "debit", 500, "USD")
        .await
        .unwrap();
    insert_entry(&mut *tx, 1, 1, 2000, "credit", 500, "USD")
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

const TABLES: [&str; 3] = ["transactions", "entries", "entry_tags"];

/// Stores one valid transaction with a tag on its first entry.
async fn insert_valid_with_tag(pool: &PgPool) {
    insert_valid(pool, 1, "valid").await;
    sqlx::query("INSERT INTO entry_tags (transaction_id, position, kind, value) VALUES (1, 0, 'customer', 'a')")
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test]
async fn refuses_update(pool: PgPool) {
    insert_valid_with_tag(&pool).await;
    for (table, set) in [
        ("transactions", "description = 'changed'"),
        ("entries", "amount = 1"),
        ("entry_tags", "value = 'changed'"),
    ] {
        let result = sqlx::query(AssertSqlSafe(format!("UPDATE {table} SET {set}")))
            .execute(&pool)
            .await
            .map(|_| ());
        assert!(refusal(result).contains("append-only"), "{table}");
    }
}

#[sqlx::test]
async fn refuses_delete(pool: PgPool) {
    insert_valid_with_tag(&pool).await;
    for table in TABLES {
        let result = sqlx::query(AssertSqlSafe(format!("DELETE FROM {table}")))
            .execute(&pool)
            .await
            .map(|_| ());
        assert!(refusal(result).contains("append-only"), "{table}");
    }
}

#[sqlx::test]
async fn refuses_truncate(pool: PgPool) {
    insert_valid_with_tag(&pool).await;
    for table in TABLES {
        let result = sqlx::query(AssertSqlSafe(format!("TRUNCATE {table} CASCADE")))
            .execute(&pool)
            .await
            .map(|_| ());
        assert!(refusal(result).contains("append-only"), "{table}");
    }
}

#[sqlx::test]
async fn refuses_unbalanced_transaction_at_commit(pool: PgPool) {
    let mut tx = pool.begin().await.unwrap();
    begin_transaction_row(&mut tx, 1, "key").await;
    insert_entry(&mut *tx, 1, 0, 1000, "debit", 2160, "USD")
        .await
        .unwrap();
    insert_entry(&mut *tx, 1, 1, 2000, "credit", 2000, "USD")
        .await
        .unwrap();
    let result = tx.commit().await;
    assert!(refusal(result).contains("not balanced"));
    assert_empty(&pool).await;
}

#[sqlx::test]
async fn refuses_single_entry_transaction_at_commit(pool: PgPool) {
    let mut tx = pool.begin().await.unwrap();
    begin_transaction_row(&mut tx, 1, "key").await;
    insert_entry(&mut *tx, 1, 0, 1000, "debit", 500, "USD")
        .await
        .unwrap();
    let result = tx.commit().await;
    assert!(refusal(result).contains("at least two entries"));
    assert_empty(&pool).await;
}

#[sqlx::test]
async fn refuses_transaction_without_entries_at_commit(pool: PgPool) {
    let mut tx = pool.begin().await.unwrap();
    begin_transaction_row(&mut tx, 1, "key").await;
    let result = tx.commit().await;
    assert!(refusal(result).contains("at least two entries"));
    assert_empty(&pool).await;
}

#[sqlx::test]
async fn accepts_balanced_three_entry_transaction(pool: PgPool) {
    let mut tx = pool.begin().await.unwrap();
    begin_transaction_row(&mut tx, 1, "key").await;
    insert_entry(&mut *tx, 1, 0, 1000, "debit", 2160, "USD")
        .await
        .unwrap();
    insert_entry(&mut *tx, 1, 1, 2000, "credit", 2000, "USD")
        .await
        .unwrap();
    insert_entry(&mut *tx, 1, 2, 3000, "credit", 160, "USD")
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

#[sqlx::test]
async fn refuses_entry_that_unbalances_a_committed_transaction(pool: PgPool) {
    insert_valid(&pool, 1, "valid").await;
    let mut tx = pool.begin().await.unwrap();
    insert_entry(&mut *tx, 1, 2, 3000, "debit", 100, "USD")
        .await
        .unwrap();
    let result = tx.commit().await;
    assert!(refusal(result).contains("not balanced"));
    let entries: i64 = sqlx::query_scalar("SELECT count(*) FROM entries WHERE transaction_id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(entries, 2);
}

#[sqlx::test]
async fn refuses_credits_only_transaction(pool: PgPool) {
    let mut tx = pool.begin().await.unwrap();
    begin_transaction_row(&mut tx, 1, "key").await;
    insert_entry(&mut *tx, 1, 0, 1000, "credit", 500, "USD")
        .await
        .unwrap();
    insert_entry(&mut *tx, 1, 1, 2000, "credit", 500, "USD")
        .await
        .unwrap();
    let result = tx.commit().await;
    assert!(refusal(result).contains("not balanced"));
    assert_empty(&pool).await;
}
