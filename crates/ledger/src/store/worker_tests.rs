//! The balance worker against a real database, one fresh database per test.

use std::num::NonZeroU32;

use sqlx::PgPool;

use super::Store;
use super::error::WorkerError;
use super::save::Attempt;
use super::store_tests::{build, build_in, clock_time, effective, store, store_at};
use super::worker::{BalanceWorker, BatchReport, WorkerConfig};
use crate::account::{Account, AccountNumber, AccountType, Side};
use crate::chart::Chart;
use crate::money::Usd;
use crate::test_support::{ASSET, REVENUE, TestEntry, TestTag, chart, key};
use crate::transaction::UnpostedTransaction;

const CASH: AccountNumber = AccountNumber::new(1010);
const DEFERRED_REVENUE: AccountNumber = AccountNumber::new(2000);
const TAX_PAYABLE: AccountNumber = AccountNumber::new(2100);
const EARNED_REVENUE: AccountNumber = AccountNumber::new(4000);

/// The four accounts of the design doc's batch example.
fn doc_chart() -> Chart {
    Chart::new([
        Account::new(CASH, "Cash", AccountType::Asset),
        Account::new(DEFERRED_REVENUE, "Deferred revenue", AccountType::Liability),
        Account::new(TAX_PAYABLE, "Tax payable", AccountType::Liability),
        Account::new(EARNED_REVENUE, "Earned revenue", AccountType::Revenue),
    ])
    .unwrap()
}

fn worker(pool: &PgPool, batch_size: u32) -> BalanceWorker {
    BalanceWorker::new(
        pool.clone(),
        WorkerConfig {
            batch_size: NonZeroU32::new(batch_size).unwrap(),
            ..WorkerConfig::default()
        },
    )
}

fn report(transactions: u32) -> BatchReport {
    BatchReport { transactions }
}

/// Debit asset, credit revenue, 100 cents, with `Alpha(1)` on the debit.
fn base_entries() -> Vec<TestEntry> {
    vec![
        (ASSET, Side::Debit, 100, vec![TestTag::Alpha(1)]),
        (REVENUE, Side::Credit, 100, vec![]),
    ]
}

fn base_keyed(key_text: &str) -> UnpostedTransaction<Usd, TestTag> {
    build(key_text, "Renewal", effective(), &base_entries())
}

fn base() -> UnpostedTransaction<Usd, TestTag> {
    base_keyed("renewal-1")
}

/// The stored (debit, credit) totals of `account`; (0, 0) if it has no row.
async fn totals(pool: &PgPool, account: AccountNumber) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT debit_total, credit_total FROM account_balances WHERE account = $1",
    )
    .bind(i64::from(account.get()))
    .fetch_optional(pool)
    .await
    .unwrap()
    .unwrap_or((0, 0))
}

/// The stored (debit, credit) totals of `account` and tag `kind`=`value`;
/// (0, 0) if it has no row.
async fn tag_totals(pool: &PgPool, account: AccountNumber, kind: &str, value: &str) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT debit_total, credit_total FROM tag_balances \
         WHERE account = $1 AND kind = $2 AND value = $3",
    )
    .bind(i64::from(account.get()))
    .bind(kind)
    .bind(value)
    .fetch_optional(pool)
    .await
    .unwrap()
    .unwrap_or((0, 0))
}

/// The checkpoint as (db_tx_id, transaction_id).
async fn checkpoint(pool: &PgPool) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT db_tx_id::text::bigint, transaction_id FROM balance_checkpoint",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The database transaction id that wrote ledger transaction `id`.
async fn db_tx_id_of(pool: &PgPool, id: i64) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT db_tx_id::text::bigint FROM transactions WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn balance_row_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT (SELECT count(*) FROM account_balances) + (SELECT count(*) FROM tag_balances)",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Saves `transaction` and returns its id.
async fn save(store: &Store<TestTag>, transaction: &UnpostedTransaction<Usd, TestTag>) -> i64 {
    let id = store
        .save(transaction)
        .await
        .unwrap()
        .into_transaction()
        .id()
        .get();
    i64::try_from(id).unwrap()
}

#[sqlx::test]
async fn design_doc_batch_example(pool: PgPool) {
    sqlx::query(
        "INSERT INTO recorded_accounts (number, account_type, contra) VALUES \
         (1010, 'asset', false), (2000, 'liability', false), \
         (2100, 'liability', false), (4000, 'revenue', false)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO account_balances (account, debit_total, credit_total) VALUES \
         (2000, 0, 500000), (4000, 0, 100000), (1010, 30000, 0), (2100, 0, 5000)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("SELECT setval('transaction_ids', 500)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE balance_checkpoint SET db_tx_id = '0', transaction_id = 500")
        .execute(&pool)
        .await
        .unwrap();

    let chart = doc_chart();
    let store = store_at(&pool, doc_chart(), clock_time());
    let batch: [(&str, Vec<TestEntry>); 3] = [
        (
            "t-501",
            vec![
                (DEFERRED_REVENUE, Side::Debit, 2_000, vec![]),
                (EARNED_REVENUE, Side::Credit, 2_000, vec![]),
            ],
        ),
        (
            "t-502",
            vec![
                (DEFERRED_REVENUE, Side::Debit, 1_000, vec![]),
                (EARNED_REVENUE, Side::Credit, 1_000, vec![]),
            ],
        ),
        (
            "t-503",
            vec![
                (CASH, Side::Debit, 2_160, vec![]),
                (DEFERRED_REVENUE, Side::Credit, 2_000, vec![]),
                (TAX_PAYABLE, Side::Credit, 160, vec![]),
            ],
        ),
    ];
    let mut ids = Vec::new();
    for (key_text, entries) in &batch {
        let transaction = build_in(&chart, key_text, "Example", effective(), entries);
        ids.push(save(&store, &transaction).await);
    }
    assert_eq!(ids, [501, 502, 503]);

    assert_eq!(worker(&pool, 1_000).run_batch().await.unwrap(), report(3));

    let deferred = totals(&pool, DEFERRED_REVENUE).await;
    assert_eq!(deferred, (3_000, 502_000));
    // Net credit: 502,000 - 3,000 = 499,000 cents ($4,990.00), down from $5,000.00.
    assert_eq!(deferred.1 - deferred.0, 499_000);
    assert_eq!(totals(&pool, EARNED_REVENUE).await, (0, 103_000));
    assert_eq!(totals(&pool, CASH).await, (32_160, 0));
    assert_eq!(totals(&pool, TAX_PAYABLE).await, (0, 5_160));
    assert_eq!(
        checkpoint(&pool).await,
        (db_tx_id_of(&pool, 503).await, 503)
    );
}

#[sqlx::test]
async fn empty_log_leaves_the_checkpoint_alone(pool: PgPool) {
    assert_eq!(worker(&pool, 1_000).run_batch().await.unwrap(), report(0));
    assert_eq!(checkpoint(&pool).await, (0, 0));
}

#[sqlx::test]
async fn batch_size_caps_the_batch(pool: PgPool) {
    let store = store(&pool);
    for n in 1..=5 {
        save(&store, &base_keyed(&format!("renewal-{n}"))).await;
    }
    let worker = worker(&pool, 2);
    let mut reports = Vec::new();
    for _ in 0..4 {
        reports.push(worker.run_batch().await.unwrap());
    }
    assert_eq!(reports, [report(2), report(2), report(1), report(0)]);
    assert_eq!(totals(&pool, ASSET).await, (500, 0));
}

#[sqlx::test]
async fn same_database_transaction_spans_a_batch_boundary(pool: PgPool) {
    let store = store(&pool);
    let mut tx = pool.begin().await.unwrap();
    for key_text in ["renewal-1", "renewal-2"] {
        let attempt = store
            .insert_on(&mut tx, &base_keyed(key_text), clock_time())
            .await
            .unwrap();
        assert!(matches!(attempt, Attempt::Inserted(_)));
    }
    tx.commit().await.unwrap();
    let shared: i64 = sqlx::query_scalar("SELECT count(DISTINCT db_tx_id) FROM transactions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(shared, 1, "both should share one database transaction");

    let worker = worker(&pool, 1);
    let mut reports = Vec::new();
    for _ in 0..3 {
        reports.push(worker.run_batch().await.unwrap());
    }
    assert_eq!(reports, [report(1), report(1), report(0)]);
    assert_eq!(totals(&pool, ASSET).await, (200, 0));
    assert_eq!(totals(&pool, REVENUE).await, (0, 200));
    assert_eq!(tag_totals(&pool, ASSET, "alpha", "1").await, (200, 0));
}

#[sqlx::test]
async fn reversal_nets_to_zero_but_both_totals_grow(pool: PgPool) {
    let store = store(&pool);
    let original = store.save(&base()).await.unwrap().into_transaction();
    let reversal = original
        .reversal(
            key("reverse-1"),
            effective(),
            "Reverse",
            &chart(),
            clock_time(),
        )
        .unwrap();
    store.save(&reversal).await.unwrap();

    assert_eq!(worker(&pool, 1_000).catch_up().await.unwrap(), 2);
    for account in [ASSET, REVENUE] {
        let (debit, credit) = totals(&pool, account).await;
        assert_eq!((debit, credit), (100, 100), "account {}", account.get());
        assert_eq!(debit - credit, 0);
    }
}

#[sqlx::test]
async fn tag_balances_follow_entry_tags(pool: PgPool) {
    save(&store(&pool), &base()).await;
    assert_eq!(worker(&pool, 1_000).run_batch().await.unwrap(), report(1));
    assert_eq!(tag_totals(&pool, ASSET, "alpha", "1").await, (100, 0));
    let beta: i64 = sqlx::query_scalar("SELECT count(*) FROM tag_balances WHERE kind = 'beta'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(beta, 0);
}

#[sqlx::test]
async fn failed_checkpoint_update_rolls_back_the_balances(pool: PgPool) {
    save(&store(&pool), &base()).await;
    sqlx::raw_sql(
        "CREATE FUNCTION fail_checkpoint() RETURNS trigger LANGUAGE plpgsql AS \
         $$ BEGIN RAISE EXCEPTION 'injected checkpoint failure'; END $$; \
         CREATE TRIGGER fail_checkpoint BEFORE UPDATE ON balance_checkpoint \
         FOR EACH ROW EXECUTE FUNCTION fail_checkpoint();",
    )
    .execute(&pool)
    .await
    .unwrap();

    let worker = worker(&pool, 1_000);
    let result = worker.run_batch().await;
    assert!(
        matches!(result, Err(WorkerError::Unavailable(_))),
        "expected Unavailable, got {result:?}"
    );
    assert_eq!(balance_row_count(&pool).await, 0);
    assert_eq!(checkpoint(&pool).await, (0, 0));

    sqlx::query("DROP TRIGGER fail_checkpoint ON balance_checkpoint")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(worker.run_batch().await.unwrap(), report(1));
    assert_eq!(totals(&pool, ASSET).await, (100, 0));
    assert_eq!(totals(&pool, REVENUE).await, (0, 100));
}

#[sqlx::test]
async fn abandoned_batch_changes_nothing(pool: PgPool) {
    save(&store(&pool), &base()).await;
    let worker = worker(&pool, 1_000);
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(worker.batch_on(&mut tx).await.unwrap(), report(1));
    drop(tx);

    assert_eq!(balance_row_count(&pool).await, 0);
    assert_eq!(checkpoint(&pool).await, (0, 0));

    assert_eq!(worker.run_batch().await.unwrap(), report(1));
    assert_eq!(totals(&pool, ASSET).await, (100, 0));
    assert_eq!(totals(&pool, REVENUE).await, (0, 100));
}

#[sqlx::test]
async fn overflow_stops_with_the_transaction_named(pool: PgPool) {
    let id = save(&store(&pool), &base()).await;
    sqlx::query(
        "INSERT INTO account_balances (account, debit_total, credit_total) VALUES ($1, $2, 0)",
    )
    .bind(i64::from(ASSET.get()))
    .bind(i64::MAX - 50)
    .execute(&pool)
    .await
    .unwrap();

    let result = worker(&pool, 1_000).run_batch().await;
    let Err(error @ WorkerError::Overflow { transaction }) = result else {
        panic!("expected Overflow, got {result:?}");
    };
    assert_eq!(transaction, id);
    assert!(!error.is_retryable());
    let message = error.to_string();
    assert!(message.contains(&format!("transaction {id}")), "{message}");

    assert_eq!(totals(&pool, ASSET).await, (i64::MAX - 50, 0));
    assert_eq!(balance_row_count(&pool).await, 1);
    assert_eq!(checkpoint(&pool).await, (0, 0));
}
