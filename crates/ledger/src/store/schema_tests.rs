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

/// Records an account, the way the ledger does on its first entry.
async fn record_account(
    executor: impl PgExecutor<'_>,
    number: i64,
    account_type: &str,
    contra: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO recorded_accounts (number, account_type, contra) VALUES ($1, $2, $3)")
        .bind(number)
        .bind(account_type)
        .bind(contra)
        .execute(executor)
        .await
        .map(|_| ())
}

/// Records accounts 1000 and 2000, the ones `begin_transaction_row` tests use.
async fn record_standard_accounts(pool: &PgPool) {
    record_account(pool, 1000, "asset", false).await.unwrap();
    record_account(pool, 2000, "liability", false)
        .await
        .unwrap();
}

/// A valid transaction with two balanced entries, committed. Records accounts
/// 1000 and 2000 first.
async fn insert_valid(pool: &PgPool, id: i64, key: &str) {
    record_standard_accounts(pool).await;
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
async fn rejects_second_reversal_of_one_transaction(pool: PgPool) {
    insert_valid(&pool, 1, "original").await;
    let mut tx = pool.begin().await.unwrap();
    insert_transaction(
        &mut *tx,
        2,
        "r1",
        "2026-01-01T00:00:00Z",
        "2026-01-02T00:00:00Z",
        Some(1),
    )
    .await
    .unwrap();
    let result = insert_transaction(
        &mut *tx,
        3,
        "r2",
        "2026-01-01T00:00:00Z",
        "2026-01-02T00:00:00Z",
        Some(1),
    )
    .await;
    assert_eq!(violated(result), "transactions_reverses_unique");
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
    record_account(&pool, 4_294_967_295, "asset", false)
        .await
        .unwrap();
    record_account(&pool, 2000, "liability", false)
        .await
        .unwrap();
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

const TABLES: [&str; 4] = ["transactions", "entries", "entry_tags", "recorded_accounts"];

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
        ("recorded_accounts", "contra = true"),
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
    record_standard_accounts(&pool).await;
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
    record_standard_accounts(&pool).await;
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
    record_standard_accounts(&pool).await;
    record_account(&pool, 3000, "revenue", false).await.unwrap();
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
    record_account(&pool, 3000, "revenue", false).await.unwrap();
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
    record_standard_accounts(&pool).await;
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

#[sqlx::test]
async fn rejects_entry_for_unrecorded_account(pool: PgPool) {
    record_account(&pool, 1000, "asset", false).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    begin_transaction_row(&mut tx, 1, "key").await;
    let result = insert_entry(&mut *tx, 1, 0, 5000, "debit", 500, "USD").await;
    assert_eq!(violated(result), "entries_account_fkey");
}

#[sqlx::test]
async fn rejects_unknown_account_type(pool: PgPool) {
    let result = record_account(&pool, 1, "cash", false).await;
    assert_eq!(violated(result), "recorded_accounts_type_valid");
}

#[sqlx::test]
async fn rejects_recorded_account_number_out_of_range(pool: PgPool) {
    let result = record_account(&pool, -1, "asset", false).await;
    assert_eq!(violated(result), "recorded_accounts_number_range");
}
