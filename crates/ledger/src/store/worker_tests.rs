//! The balance worker against a real database, one fresh database per test.

use std::cell::Cell;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use proptest::prelude::prop;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;
use sqlx::PgPool;

use super::error::{StoredBalanceError, WorkerError};
use super::save::Attempt;
use super::store_tests::{
    base, base_entries, base_keyed, build, build_in, clock_time, effective, hold_open, poll_until,
    store, store_at, wait_for_lock_waiter, waiting_query,
};
use super::worker::{BalanceWorker, BatchReport, WorkerConfig};
use super::{CorruptData, Store, StoredTag};
use crate::account::{Account, AccountNumber, AccountType, Side};
use crate::balance::{BalanceError, BalanceQuery, balance};
use crate::chart::Chart;
use crate::money::Usd;
use crate::tag::Tag;
use crate::test_support::{
    ASSET, CONTRA_REVENUE, LIABILITY, NOT_IN_CHART, REVENUE, TestEntry, TestTag, chart, key, usd,
    valid_entries_with_varied_tags,
};
use crate::transaction::{PostedTransaction, UnpostedTransaction};

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

/// The ledger transaction id of `posted`, as the database stores it.
fn id_of(posted: &PostedTransaction<Usd, TestTag>) -> i64 {
    i64::try_from(posted.id().get()).unwrap()
}

/// The highest ledger transaction id in the log.
async fn last_transaction_id(pool: &PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT max(id) FROM transactions")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Asserts the checkpoint is at ledger transaction `id` and the database
/// transaction that wrote it.
async fn assert_checkpoint_at(pool: &PgPool, id: i64) {
    assert_eq!(checkpoint(pool).await, (db_tx_id_of(pool, id).await, id));
}

/// Waits until the safe line has passed every committed posting in this
/// database, so the next batch may count them all. The rule it waits for is
/// the one in `batch_on`, which is the source of truth.
///
/// Tests need this bounded wait because parallel test databases share the
/// server's safe line: it is cluster-wide, so another test's open transaction
/// (in its own database) holds this test's postings back too. The wait ends
/// as soon as those transactions finish.
async fn settle(pool: &PgPool) {
    poll_until(
        "the safe line passes this database's postings",
        async || {
            sqlx::query_scalar::<_, bool>(
                "SELECT coalesce(max(db_tx_id), '0'::xid8) \
                    < pg_snapshot_xmin(pg_current_snapshot()) FROM transactions",
            )
            .fetch_one(pool)
            .await
            .unwrap()
        },
    )
    .await;
}

/// Begins a database transaction and makes it take a transaction id, so it
/// holds the safe line back until it ends.
async fn hold_safe_line(pool: &PgPool) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_current_xact_id()")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

/// Saves `transaction` and returns its id.
async fn save(store: &Store<TestTag>, transaction: &UnpostedTransaction<Usd, TestTag>) -> i64 {
    id_of(&store.save(transaction).await.unwrap().into_transaction())
}

/// Saves the base transaction `n` times, keyed `renewal-1` to `renewal-n`,
/// and returns their ids.
async fn save_base(store: &Store<TestTag>, n: u32) -> Vec<i64> {
    let mut ids = Vec::new();
    for k in 1..=n {
        ids.push(save(store, &base_keyed(&format!("renewal-{k}"))).await);
    }
    ids
}

/// Asserts the stored totals are those of `n` base transactions: `ASSET`
/// debited, `REVENUE` credited and `ASSET`'s `Alpha(1)` tag debited, 100
/// cents each time.
async fn assert_base_counted(pool: &PgPool, n: i64) {
    let cents = 100 * n;
    let tag = TestTag::Alpha(1);
    assert_eq!(totals(pool, ASSET).await, (cents, 0), "ASSET");
    assert_eq!(totals(pool, REVENUE).await, (0, cents), "REVENUE");
    assert_eq!(
        tag_totals(pool, ASSET, tag.kind(), &tag.value()).await,
        (cents, 0),
        "ASSET tagged {tag:?}"
    );
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
    let store = store_at(&pool, chart.clone(), clock_time());
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

    settle(&pool).await;
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
    save_base(&store(&pool), 5).await;
    let worker = worker(&pool, 2);
    settle(&pool).await;
    let mut reports = Vec::new();
    for _ in 0..4 {
        reports.push(worker.run_batch().await.unwrap());
    }
    assert_eq!(reports, [report(2), report(2), report(1), report(0)]);
    assert_base_counted(&pool, 5).await;
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

    settle(&pool).await;
    let worker = worker(&pool, 1);
    assert_eq!(worker.run_batch().await.unwrap(), report(1));
    assert_eq!(
        worker.lag().await.unwrap(),
        1,
        "the second is still to count"
    );
    assert_eq!(worker.run_batch().await.unwrap(), report(1));
    assert_eq!(worker.run_batch().await.unwrap(), report(0));
    assert_eq!(worker.lag().await.unwrap(), 0);
    assert_base_counted(&pool, 2).await;
}

/// Inserts the base transaction keyed `key_text` inside the open `tx` and
/// returns its id.
async fn insert_in(
    store: &Store<TestTag>,
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    key_text: &str,
) -> i64 {
    let Attempt::Inserted(posted) = store
        .insert_on(tx, &base_keyed(key_text), clock_time())
        .await
        .unwrap()
    else {
        panic!("{key_text} should have inserted");
    };
    id_of(&posted)
}

#[sqlx::test]
async fn log_order_is_transaction_id_then_row_id_not_row_id_alone(pool: PgPool) {
    record_base_accounts(&pool).await;
    let store = store(&pool);
    // X takes its database transaction id first, so its id is the lower one.
    let mut x = hold_safe_line(&pool).await;
    let mut y = hold_safe_line(&pool).await;
    // Y inserts and commits first, so it draws the lower ledger transaction id.
    let y_id = insert_in(&store, &mut y, "renewal-y").await;
    y.commit().await.unwrap();
    let x_id = insert_in(&store, &mut x, "renewal-x").await;
    x.commit().await.unwrap();
    assert!(y_id < x_id, "Y has the lower ledger transaction id");
    assert!(
        db_tx_id_of(&pool, x_id).await < db_tx_id_of(&pool, y_id).await,
        "X has the lower database transaction id"
    );

    settle(&pool).await;
    let worker = worker(&pool, 1);
    assert_eq!(worker.run_batch().await.unwrap(), report(1));
    assert_checkpoint_at(&pool, x_id).await;
    assert_eq!(worker.lag().await.unwrap(), 1, "Y is still to count");

    assert_eq!(worker.run_batch().await.unwrap(), report(1));
    assert_checkpoint_at(&pool, y_id).await;
    assert_eq!(worker.run_batch().await.unwrap(), report(0));
    assert_base_counted(&pool, 2).await;
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

    settle(&pool).await;
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
    settle(&pool).await;
    assert_eq!(worker(&pool, 1_000).run_batch().await.unwrap(), report(1));
    assert_base_counted(&pool, 1).await;
    let beta: i64 = sqlx::query_scalar("SELECT count(*) FROM tag_balances WHERE kind = 'beta'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(beta, 0);
}

/// Makes every checkpoint update raise an exception (SQLSTATE `P0001`).
async fn fail_checkpoint_updates(pool: &PgPool) {
    sqlx::raw_sql(
        "CREATE FUNCTION fail_checkpoint() RETURNS trigger LANGUAGE plpgsql AS \
         $$ BEGIN RAISE EXCEPTION 'injected checkpoint failure'; END $$; \
         CREATE TRIGGER fail_checkpoint BEFORE UPDATE ON balance_checkpoint \
         FOR EACH ROW EXECUTE FUNCTION fail_checkpoint();",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test]
async fn failed_checkpoint_update_rolls_back_the_balances(pool: PgPool) {
    let id = save(&store(&pool), &base()).await;
    fail_checkpoint_updates(&pool).await;

    settle(&pool).await;
    let worker = worker(&pool, 1_000);
    let result = worker.run_batch().await;
    let Err(
        ref error @ WorkerError::Rejected {
            transactions: Some(batch),
            ..
        },
    ) = result
    else {
        panic!("expected Rejected naming the batch, got {result:?}");
    };
    assert_eq!(batch, (id, id));
    assert!(!error.is_retryable());
    assert_eq!(balance_row_count(&pool).await, 0);
    assert_eq!(checkpoint(&pool).await, (0, 0));

    sqlx::query("DROP TRIGGER fail_checkpoint ON balance_checkpoint")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(worker.run_batch().await.unwrap(), report(1));
    assert_base_counted(&pool, 1).await;
}

#[sqlx::test]
async fn abandoned_batch_changes_nothing(pool: PgPool) {
    save(&store(&pool), &base()).await;
    settle(&pool).await;
    let worker = worker(&pool, 1_000);
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(worker.batch_on(&mut tx).await.unwrap(), report(1));
    drop(tx);

    assert_eq!(balance_row_count(&pool).await, 0);
    assert_eq!(checkpoint(&pool).await, (0, 0));

    assert_eq!(worker.run_batch().await.unwrap(), report(1));
    assert_base_counted(&pool, 1).await;
}

/// Saves the base transaction and stores an `ASSET` total so close to the
/// limit that counting it would overflow. Returns the transaction's id.
async fn overflowing_setup(pool: &PgPool) -> i64 {
    let id = save(&store(pool), &base()).await;
    sqlx::query(
        "INSERT INTO account_balances (account, debit_total, credit_total) VALUES ($1, $2, 0)",
    )
    .bind(i64::from(ASSET.get()))
    .bind(i64::MAX - 50)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[sqlx::test]
async fn overflow_stops_with_the_transaction_named(pool: PgPool) {
    let id = overflowing_setup(&pool).await;

    settle(&pool).await;
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

/// Writes a ledger transaction straight to the tables, bypassing the store,
/// and returns its id. `entries` are (position, account, side, cents) and
/// `tags` are (position, kind, value). A test writing a row the schema would
/// refuse drops that constraint first.
async fn insert_raw(
    pool: &PgPool,
    key_text: &str,
    entries: &[(i16, AccountNumber, &str, i64)],
    tags: &[(i16, &str, &str)],
) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    let id: i64 = sqlx::query_scalar("SELECT nextval('transaction_ids')")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO transactions (id, idempotency_key, description, effective_at, recorded_at) \
         VALUES ($1, $2, 'Raw', $3, $4)",
    )
    .bind(id)
    .bind(key_text)
    .bind(effective())
    .bind(clock_time())
    .execute(&mut *tx)
    .await
    .unwrap();
    for &(position, account, side, cents) in entries {
        sqlx::query(
            "INSERT INTO entries (transaction_id, position, account, side, amount, currency) \
             VALUES ($1, $2, $3, $4, $5, 'USD')",
        )
        .bind(id)
        .bind(position)
        .bind(i64::from(account.get()))
        .bind(side)
        .bind(cents)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    for &(position, kind, value) in tags {
        sqlx::query(
            "INSERT INTO entry_tags (transaction_id, position, kind, value) VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(position)
        .bind(kind)
        .bind(value)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    id
}

/// Counts one good transaction, then writes the bad one `insert_bad` makes and
/// asserts the worker stops on it with `expected` (which must name the
/// transaction), changing nothing, both from `run_batch` and from `run`.
async fn assert_corrupt_row_stops_the_worker(
    pool: &PgPool,
    insert_bad: impl AsyncFnOnce() -> i64,
    expected: impl Fn(i64) -> CorruptData,
) {
    save(&store(pool), &base()).await;
    settle(pool).await;
    let worker = worker(pool, 1_000);
    assert_eq!(worker.catch_up().await.unwrap(), 1);
    let before = snapshot(pool).await;
    let end = checkpoint(pool).await;

    let bad = insert_bad().await;
    settle(pool).await;
    let result = worker.run_batch().await;
    let Err(ref error @ WorkerError::Corrupt(ref corrupt)) = result else {
        panic!("expected Corrupt, got {result:?}");
    };
    assert_eq!(*corrupt, expected(bad));
    assert!(!error.is_retryable());
    let message = error.to_string();
    assert!(message.contains(&format!("transaction {bad}")), "{message}");
    assert_eq!(snapshot(pool).await, before);
    assert_eq!(checkpoint(pool).await, end);

    let (pause, waits) = recording_pause();
    let stopped = worker.run(|| false, pause).await;
    let Err(WorkerError::Corrupt(ref again)) = stopped else {
        panic!("expected run to stop with Corrupt, got {stopped:?}");
    };
    assert_eq!(*again, expected(bad));
    assert!(waits_of(&waits).is_empty());
    assert_eq!(snapshot(pool).await, before);
    assert_eq!(checkpoint(pool).await, end);
}

#[sqlx::test]
async fn unknown_side_stops_the_worker_and_is_not_skipped(pool: PgPool) {
    sqlx::query("ALTER TABLE entries DROP CONSTRAINT entries_side_valid")
        .execute(&pool)
        .await
        .unwrap();
    // Balanced on the debit and credit sides, plus an entry on neither.
    let insert_bad = async || {
        insert_raw(
            &pool,
            "sideways-1",
            &[
                (0, ASSET, "debit", 100),
                (1, REVENUE, "credit", 100),
                (2, ASSET, "sideways", 50),
            ],
            &[],
        )
        .await
    };
    assert_corrupt_row_stops_the_worker(&pool, insert_bad, |transaction| {
        CorruptData::UnknownSide {
            transaction,
            side: "sideways".to_owned(),
        }
    })
    .await;
}

#[sqlx::test]
async fn orphan_tag_stops_the_worker_and_is_not_skipped(pool: PgPool) {
    sqlx::query("ALTER TABLE entry_tags DROP CONSTRAINT entry_tags_entry_fkey")
        .execute(&pool)
        .await
        .unwrap();
    // A tag on position 5 of a transaction with entries at 0 and 1 only.
    let insert_bad = async || {
        insert_raw(
            &pool,
            "orphan-1",
            &[(0, ASSET, "debit", 100), (1, REVENUE, "credit", 100)],
            &[(5, "alpha", "1")],
        )
        .await
    };
    assert_corrupt_row_stops_the_worker(&pool, insert_bad, |transaction| CorruptData::OutOfRange {
        transaction,
        column: "tag position",
        value: 5,
    })
    .await;
}

/// Records the base accounts up front. Otherwise a held-open first use of
/// them makes a later save wait on its uncommitted `recorded_accounts` row.
async fn record_base_accounts(pool: &PgPool) {
    sqlx::query(
        "INSERT INTO recorded_accounts (number, account_type, contra) VALUES \
         ($1, 'asset', false), ($2, 'revenue', false)",
    )
    .bind(i64::from(ASSET.get()))
    .bind(i64::from(REVENUE.get()))
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test]
async fn later_posting_finishing_first_is_not_skipped(pool: PgPool) {
    record_base_accounts(&pool).await;
    let store = store(&pool);
    let (tx_a, a) = hold_open(&pool, &store, &base_keyed("renewal-a")).await;
    let b = save(&store, &base_keyed("renewal-b")).await;
    assert!(id_of(&a) < b, "A started first, so it has the lower id");

    let worker = worker(&pool, 1_000);
    assert_eq!(worker.run_batch().await.unwrap(), report(0));
    assert_eq!(balance_row_count(&pool).await, 0);

    tx_a.commit().await.unwrap();
    settle(&pool).await;
    assert_eq!(worker.catch_up().await.unwrap(), 2);
    assert_base_counted(&pool, 2).await;
    assert_checkpoint_at(&pool, b).await;
}

#[sqlx::test]
async fn partial_batch_counts_postings_below_the_safe_line(pool: PgPool) {
    record_base_accounts(&pool).await;
    let store = store(&pool);
    let t1 = save(&store, &base_keyed("renewal-1")).await;
    settle(&pool).await;
    // A stays open, so the safe line stops at it; B commits above the line.
    let (tx_a, _) = hold_open(&pool, &store, &base_keyed("renewal-a")).await;
    save(&store, &base_keyed("renewal-b")).await;

    let worker = worker(&pool, 1_000);
    assert_eq!(worker.run_batch().await.unwrap(), report(1));
    assert_checkpoint_at(&pool, t1).await;
    assert_base_counted(&pool, 1).await;
    assert_eq!(
        worker.lag().await.unwrap(),
        1,
        "B is committed lag; A isn't committed yet"
    );

    tx_a.commit().await.unwrap();
    settle(&pool).await;
    assert_eq!(worker.catch_up().await.unwrap(), 2);
    assert_base_counted(&pool, 3).await;
}

#[sqlx::test]
async fn rolled_back_posting_does_not_stall_the_worker(pool: PgPool) {
    record_base_accounts(&pool).await;
    let store = store(&pool);
    let (tx_a, a) = hold_open(&pool, &store, &base_keyed("renewal-a")).await;
    let b = save(&store, &base_keyed("renewal-b")).await;
    tx_a.rollback().await.unwrap();

    settle(&pool).await;
    assert_eq!(worker(&pool, 1_000).run_batch().await.unwrap(), report(1));
    let gap: i64 = sqlx::query_scalar("SELECT count(*) FROM transactions WHERE id = $1")
        .bind(id_of(&a))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(gap, 0, "A's id stays a permanent gap");
    assert_base_counted(&pool, 1).await;
    assert_checkpoint_at(&pool, b).await;
}

#[sqlx::test]
async fn open_transaction_elsewhere_only_delays_the_worker(pool: PgPool) {
    let elsewhere = hold_safe_line(&pool).await;
    save(&store(&pool), &base()).await;

    let worker = worker(&pool, 1_000);
    assert_eq!(worker.run_batch().await.unwrap(), report(0));

    elsewhere.rollback().await.unwrap();
    settle(&pool).await;
    assert_eq!(worker.run_batch().await.unwrap(), report(1));
    assert_base_counted(&pool, 1).await;
}

#[sqlx::test]
async fn second_worker_waits_on_the_checkpoint_lock_before_reading(pool: PgPool) {
    save_base(&store(&pool), 2).await;
    settle(&pool).await;

    let mut other = pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM balance_checkpoint FOR UPDATE")
        .execute(&mut *other)
        .await
        .unwrap();
    let second = worker(&pool, 1_000);
    let batch = tokio::spawn(async move { second.run_batch().await });
    wait_for_lock_waiter(&pool).await;

    let waiting = waiting_query(&pool).await;
    assert!(waiting.contains("balance_checkpoint"), "{waiting}");
    assert!(waiting.contains("FOR UPDATE"), "{waiting}");

    other.commit().await.unwrap();
    assert_eq!(batch.await.unwrap().unwrap(), report(2));
    assert_base_counted(&pool, 2).await;
}

#[sqlx::test]
async fn batch_started_while_another_commits_counts_once(pool: PgPool) {
    save_base(&store(&pool), 2).await;
    settle(&pool).await;

    let first = worker(&pool, 1_000);
    let mut tx_a = pool.begin().await.unwrap();
    assert_eq!(first.batch_on(&mut tx_a).await.unwrap(), report(2));
    let second = worker(&pool, 1_000);
    let batch = tokio::spawn(async move { second.run_batch().await });
    wait_for_lock_waiter(&pool).await;

    tx_a.commit().await.unwrap();
    assert_eq!(batch.await.unwrap().unwrap(), report(0));
    assert_base_counted(&pool, 2).await;
}

/// A `pause` that records each wait instead of sleeping, and the waits so far.
fn recording_pause() -> (impl AsyncFn(Duration), Arc<Mutex<Vec<Duration>>>) {
    let waits = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&waits);
    let pause = async move |wait: Duration| log.lock().unwrap().push(wait);
    (pause, waits)
}

/// A `should_stop` that is false for the first `calls - 1` calls and true from
/// call number `calls` on.
fn stop_on_call(calls: usize) -> impl Fn() -> bool {
    let seen = Cell::new(0_usize);
    move || {
        seen.set(seen.get() + 1);
        seen.get() >= calls
    }
}

fn waits_of(waits: &Arc<Mutex<Vec<Duration>>>) -> Vec<Duration> {
    waits.lock().unwrap().clone()
}

#[sqlx::test]
async fn idle_loop_waits_then_stops(pool: PgPool) {
    let worker = worker(&pool, 1_000);
    let (pause, waits) = recording_pause();
    worker.run(stop_on_call(3), pause).await.unwrap();
    let idle = WorkerConfig::default().idle_wait;
    assert_eq!(waits_of(&waits), [idle, idle]);
}

#[sqlx::test]
async fn full_batch_goes_again_without_waiting(pool: PgPool) {
    save_base(&store(&pool), 2).await;
    settle(&pool).await;

    let worker = worker(&pool, 2);
    let (pause, waits) = recording_pause();
    worker.run(stop_on_call(3), pause).await.unwrap();
    assert_eq!(waits_of(&waits), [WorkerConfig::default().idle_wait]);
    assert_base_counted(&pool, 2).await;
}

#[sqlx::test]
async fn transient_errors_retry_with_growing_waits_then_stop(pool: PgPool) {
    let worker = BalanceWorker::new(
        pool.clone(),
        WorkerConfig {
            retries: 3,
            retry_wait: Duration::from_millis(10),
            ..WorkerConfig::default()
        },
    );
    pool.close().await;
    let (pause, waits) = recording_pause();
    let result = worker.run(|| false, pause).await;
    assert!(
        matches!(result, Err(WorkerError::Unavailable(_))),
        "expected Unavailable, got {result:?}"
    );
    assert_eq!(
        waits_of(&waits),
        [
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(40)
        ]
    );
}

#[sqlx::test]
async fn permanent_error_stops_without_retrying(pool: PgPool) {
    let id = overflowing_setup(&pool).await;
    settle(&pool).await;

    let (pause, waits) = recording_pause();
    let result = worker(&pool, 1_000).run(|| false, pause).await;
    let Err(WorkerError::Overflow { transaction }) = result else {
        panic!("expected Overflow, got {result:?}");
    };
    assert_eq!(transaction, id);
    assert!(waits_of(&waits).is_empty());
    assert_eq!(checkpoint(&pool).await, (0, 0));
}

#[sqlx::test]
async fn rejected_error_stops_the_loop_without_retrying(pool: PgPool) {
    let ids = save_base(&store(&pool), 2).await;
    fail_checkpoint_updates(&pool).await;
    settle(&pool).await;

    let (pause, waits) = recording_pause();
    let result = worker(&pool, 1_000).run(|| false, pause).await;
    let Err(WorkerError::Rejected { transactions, .. }) = result else {
        panic!("expected Rejected, got {result:?}");
    };
    assert_eq!(transactions, Some((ids[0], ids[1])));
    assert!(waits_of(&waits).is_empty());
    assert_eq!(balance_row_count(&pool).await, 0);
    assert_eq!(checkpoint(&pool).await, (0, 0));
}

#[sqlx::test]
async fn closed_pool_is_a_transient_error(pool: PgPool) {
    let worker = worker(&pool, 1_000);
    pool.close().await;
    let result = worker.run_batch().await;
    let Err(error @ WorkerError::Unavailable(_)) = result else {
        panic!("expected Unavailable, got {result:?}");
    };
    assert!(error.is_retryable());
}

#[sqlx::test]
async fn constraint_violation_is_a_permanent_error(pool: PgPool) {
    let violation = sqlx::query(
        "INSERT INTO account_balances (account, debit_total, credit_total) VALUES (1, -1, 0)",
    )
    .execute(&pool)
    .await
    .unwrap_err();
    let error = WorkerError::from_database(violation, None);
    assert!(
        matches!(
            error,
            WorkerError::Rejected {
                transactions: None,
                ..
            }
        ),
        "expected Rejected, got {error:?}"
    );
    assert!(!error.is_retryable());
}

/// The worker's classification of a database error with SQLSTATE `code`.
async fn classify(pool: &PgPool, code: &str) -> WorkerError {
    let raised = sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DO $$ BEGIN RAISE EXCEPTION 'injected' USING ERRCODE = '{code}'; END $$"
    )))
    .execute(pool)
    .await
    .unwrap_err();
    WorkerError::from_database(raised, None)
}

#[sqlx::test]
async fn sqlstate_decides_transient_or_permanent(pool: PgPool) {
    // Connection, serialization failure, deadlock, out of connections,
    // query cancelled, lock not available.
    for code in ["08006", "40001", "40P01", "53300", "57014", "55P03"] {
        let error = classify(&pool, code).await;
        assert!(error.is_retryable(), "{code}: {error:?}");
    }
    // Check violation, division by zero, raised exception, and another
    // class 55 code: only 55P03 of that class is transient.
    for code in ["23514", "22012", "P0001", "55000"] {
        let error = classify(&pool, code).await;
        assert!(
            matches!(error, WorkerError::Rejected { .. }),
            "{code}: {error:?}"
        );
    }
}

#[test]
fn rejected_names_the_batch_when_known() {
    let named = WorkerError::from_database(sqlx::Error::RowNotFound, Some((501, 503))).to_string();
    assert!(named.contains("transactions 501 to 503"), "{named}");
    let unnamed = WorkerError::from_database(sqlx::Error::RowNotFound, None).to_string();
    assert!(unnamed.starts_with("balance worker stopped: "), "{unnamed}");
}

/// Both balance tables as sorted rows, for comparing before and after.
async fn snapshot(pool: &PgPool) -> (Vec<(i64, i64, i64)>, Vec<(i64, String, String, i64, i64)>) {
    let accounts = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT account, debit_total, credit_total FROM account_balances ORDER BY account",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let tags = sqlx::query_as::<_, (i64, String, String, i64, i64)>(
        "SELECT account, kind, value, debit_total, credit_total FROM tag_balances \
         ORDER BY account, kind, value",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    (accounts, tags)
}

#[sqlx::test]
async fn reset_clears_balances_and_returns_the_checkpoint_to_the_start(pool: PgPool) {
    save_base(&store(&pool), 2).await;
    settle(&pool).await;
    let worker = worker(&pool, 1_000);
    assert_eq!(worker.catch_up().await.unwrap(), 2);
    assert!(balance_row_count(&pool).await > 0);

    worker.reset().await.unwrap();
    assert_eq!(balance_row_count(&pool).await, 0);
    assert_eq!(checkpoint(&pool).await, (0, 0));
}

#[sqlx::test]
async fn rebuild_matches_the_incremental_balances(pool: PgPool) {
    save_base(&store(&pool), 4).await;
    settle(&pool).await;
    let worker = worker(&pool, 2);
    assert_eq!(worker.catch_up().await.unwrap(), 4);
    let before = snapshot(&pool).await;
    let end = checkpoint(&pool).await;
    assert!(!before.0.is_empty() && !before.1.is_empty());

    assert_eq!(worker.rebuild().await.unwrap(), 4);
    assert_eq!(snapshot(&pool).await, before);
    assert_eq!(checkpoint(&pool).await, end);
}

#[sqlx::test]
async fn rebuild_during_a_batch_waits_for_the_lock(pool: PgPool) {
    save_base(&store(&pool), 2).await;
    settle(&pool).await;

    let first = worker(&pool, 1_000);
    let mut tx_a = pool.begin().await.unwrap();
    assert_eq!(first.batch_on(&mut tx_a).await.unwrap(), report(2));
    let second = worker(&pool, 1_000);
    let rebuild = tokio::spawn(async move { second.rebuild().await });
    wait_for_lock_waiter(&pool).await;

    tx_a.commit().await.unwrap();
    assert_eq!(rebuild.await.unwrap().unwrap(), 2);
    assert_base_counted(&pool, 2).await;
    assert_checkpoint_at(&pool, last_transaction_id(&pool).await).await;
}

#[sqlx::test]
async fn lag_counts_committed_transactions_after_the_checkpoint(pool: PgPool) {
    record_base_accounts(&pool).await;
    let store = store(&pool);
    save_base(&store, 3).await;
    settle(&pool).await;
    let worker = worker(&pool, 2);
    assert_eq!(worker.lag().await.unwrap(), 3);

    assert_eq!(worker.run_batch().await.unwrap(), report(2));
    assert_eq!(worker.lag().await.unwrap(), 1);

    let (held, _) = hold_open(&pool, &store, &base_keyed("renewal-held")).await;
    assert_eq!(
        worker.lag().await.unwrap(),
        1,
        "an open posting is not counted"
    );
    held.rollback().await.unwrap();

    assert_eq!(worker.catch_up().await.unwrap(), 1);
    assert_eq!(worker.lag().await.unwrap(), 0);
}

#[sqlx::test]
async fn lag_counts_committed_postings_held_back_by_the_safe_line(pool: PgPool) {
    record_base_accounts(&pool).await;
    let store = store(&pool);
    save(&store, &base_keyed("renewal-1")).await;
    settle(&pool).await;
    let worker = worker(&pool, 1_000);
    assert_eq!(worker.catch_up().await.unwrap(), 1);

    // An open transaction that has taken an id holds the safe line back.
    let holder = hold_safe_line(&pool).await;
    save(&store, &base_keyed("renewal-2")).await;

    assert_eq!(
        worker.lag().await.unwrap(),
        1,
        "a committed posting behind the line is lag"
    );
    assert_eq!(worker.run_batch().await.unwrap(), report(0));

    holder.rollback().await.unwrap();
    settle(&pool).await;
    assert_eq!(worker.catch_up().await.unwrap(), 1);
    assert_eq!(worker.lag().await.unwrap(), 0);
}

#[sqlx::test]
async fn batch_waiting_on_a_rebuild_counts_everything_from_the_start(pool: PgPool) {
    save_base(&store(&pool), 2).await;
    settle(&pool).await;
    let first = worker(&pool, 1_000);
    assert_eq!(first.catch_up().await.unwrap(), 2);
    assert_ne!(checkpoint(&pool).await, (0, 0));

    // A rebuild's reset, held open: the checkpoint moves back at commit.
    let mut reset = pool.begin().await.unwrap();
    first.reset_on(&mut reset).await.unwrap();
    let waiting = worker(&pool, 1_000);
    let batch = tokio::spawn(async move { waiting.run_batch().await });
    wait_for_lock_waiter(&pool).await;

    reset.commit().await.unwrap();
    assert_eq!(batch.await.unwrap().unwrap(), report(2));
    assert_base_counted(&pool, 2).await;
    assert_checkpoint_at(&pool, last_transaction_id(&pool).await).await;
}

// --- Reading stored balances ---

/// Saves `entries` as a transaction with `key_text`, settles, and catches up.
async fn save_and_catch_up(
    pool: &PgPool,
    store: &Store<TestTag>,
    key_text: &str,
    entries: &[TestEntry],
) {
    store
        .save(&build(key_text, "Stored", effective(), entries))
        .await
        .unwrap();
    settle(pool).await;
    worker(pool, 4).catch_up().await.unwrap();
}

#[sqlx::test]
async fn contra_account_reads_on_its_debit_side(pool: PgPool) {
    let store = store(&pool);
    // Debit the contra-revenue account, credit revenue: 100 cents each.
    save_and_catch_up(
        &pool,
        &store,
        "contra-1",
        &[
            (CONTRA_REVENUE, Side::Debit, 100, vec![]),
            (REVENUE, Side::Credit, 100, vec![]),
        ],
    )
    .await;
    assert_eq!(
        store.stored_balance(CONTRA_REVENUE, None).await.unwrap(),
        usd(100)
    );
    assert_eq!(store.stored_balance(REVENUE, None).await.unwrap(), usd(100));
}

#[sqlx::test]
async fn abnormal_balance_is_negative(pool: PgPool) {
    let store = store(&pool);
    // Credit an asset: its normal side is debit, so the balance is -100.
    save_and_catch_up(
        &pool,
        &store,
        "abnormal-1",
        &[
            (ASSET, Side::Credit, 100, vec![]),
            (REVENUE, Side::Debit, 100, vec![]),
        ],
    )
    .await;
    assert_eq!(store.stored_balance(ASSET, None).await.unwrap(), usd(-100));
}

#[sqlx::test]
async fn known_account_with_no_row_reads_zero(pool: PgPool) {
    let store = store(&pool);
    assert_eq!(store.stored_balance(LIABILITY, None).await.unwrap(), usd(0));
}

#[sqlx::test]
async fn tag_balance_with_no_row_reads_zero(pool: PgPool) {
    let store = store(&pool);
    save_and_catch_up(&pool, &store, "tagged-1", &base_entries()).await;
    assert_eq!(
        store
            .stored_balance(ASSET, Some(&TestTag::Alpha(1)))
            .await
            .unwrap(),
        usd(100)
    );
    assert_eq!(
        store
            .stored_balance(ASSET, Some(&TestTag::Beta(2)))
            .await
            .unwrap(),
        usd(0)
    );
}

#[sqlx::test]
async fn account_not_in_chart_is_an_error(pool: PgPool) {
    let store = store(&pool);
    match store.stored_balance(NOT_IN_CHART, None).await {
        Err(StoredBalanceError::Balance(BalanceError::UnknownAccount { account })) => {
            assert_eq!(account, NOT_IN_CHART);
        }
        other => panic!("expected UnknownAccount, got {other:?}"),
    }
}

#[sqlx::test]
async fn stored_balance_matches_balance_for_every_account_and_tag(pool: PgPool) {
    let store = store(&pool);
    let mut runner = TestRunner::deterministic();
    let transactions = prop::collection::vec(valid_entries_with_varied_tags(), 2..=5);
    let sizes = 1_u32..=4;
    let mut saved: Vec<PostedTransaction<Usd, TestTag>> = Vec::new();
    let mut counter = 0_u32;
    let accounts = [ASSET, LIABILITY, REVENUE, CONTRA_REVENUE];
    let tags: Vec<Option<TestTag>> = std::iter::once(None)
        .chain((0..3).flat_map(|n| [Some(TestTag::Alpha(n)), Some(TestTag::Beta(n))]))
        .collect();

    for _scenario in 0..8 {
        let batch = transactions.new_tree(&mut runner).unwrap().current();
        let size = sizes.new_tree(&mut runner).unwrap().current();
        for entries in &batch {
            counter += 1;
            let result = store
                .save(&build(
                    &format!("stored-{counter}"),
                    "Generated",
                    effective(),
                    entries,
                ))
                .await
                .unwrap();
            saved.push(
                store
                    .load(result.transaction().id())
                    .await
                    .unwrap()
                    .unwrap(),
            );
        }
        let context = format!("batch size {size}, scenario entries {batch:?}");
        settle(&pool).await;
        worker(&pool, size).catch_up().await.unwrap();
        assert_stored_matches(&store, &saved, &accounts, &tags, &context).await;

        settle(&pool).await;
        worker(&pool, size).rebuild().await.unwrap();
        assert_stored_matches(&store, &saved, &accounts, &tags, &context).await;
    }
}

async fn assert_stored_matches(
    store: &Store<TestTag>,
    saved: &[PostedTransaction<Usd, TestTag>],
    accounts: &[AccountNumber],
    tags: &[Option<TestTag>],
    context: &str,
) {
    for &account in accounts {
        for tag in tags {
            let query = match tag {
                Some(tag) => BalanceQuery::account(account).tagged(tag.clone()),
                None => BalanceQuery::account(account),
            };
            let expected = balance(saved, &chart(), &query).unwrap();
            let stored = store.stored_balance(account, tag.as_ref()).await.unwrap();
            assert_eq!(
                stored,
                expected,
                "account {} tag {tag:?}; {context}",
                account.get()
            );
        }
    }
}
