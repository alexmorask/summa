//! `save` and `load` against a real database, one fresh database per test.

use std::sync::Arc;

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;
use sqlx::PgPool;

use super::save::Attempt;
use super::{CorruptData, LoadError, SaveError, Saved, Store, StoredTag};
use crate::account::{Account, AccountType, Side};
use crate::chart::Chart;
use crate::clock::FixedClock;
use crate::money::Usd;
use crate::reversal::ReversalError;
use crate::tag::Tag;
use crate::test_support::{
    ASSET, LIABILITY, NOT_IN_CHART, REVENUE, TestEntry, TestTag, chart, key, usd, valid_entries,
};
use crate::transaction::{PostError, TransactionId, UnpostedTransaction};

/// 2026-10-01T12:00:00Z: the store's clock in these tests.
fn clock_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 1, 12, 0, 0).unwrap()
}

/// An effective time safely before `clock_time()`.
fn effective() -> DateTime<Utc> {
    clock_time() - TimeDelta::days(1)
}

fn store_at(pool: &PgPool, chart: Chart, now: DateTime<Utc>) -> Store<TestTag> {
    Store::new(pool.clone(), chart, Arc::new(FixedClock::new(now)))
}

fn store(pool: &PgPool) -> Store<TestTag> {
    store_at(pool, chart(), clock_time())
}

/// A transaction built from `entries`, against `chart`.
fn build_in(
    chart: &Chart,
    key_text: &str,
    description: &str,
    effective_at: DateTime<Utc>,
    entries: &[TestEntry],
) -> UnpostedTransaction<Usd, TestTag> {
    entries
        .iter()
        .cloned()
        .fold(
            UnpostedTransaction::builder(key(key_text), effective_at, description),
            |builder, (account, side, cents, tags)| match side {
                Side::Debit => builder.debit(account, usd(cents), tags),
                Side::Credit => builder.credit(account, usd(cents), tags),
            },
        )
        .build(chart, clock_time())
        .unwrap()
}

fn build(
    key_text: &str,
    description: &str,
    effective_at: DateTime<Utc>,
    entries: &[TestEntry],
) -> UnpostedTransaction<Usd, TestTag> {
    build_in(&chart(), key_text, description, effective_at, entries)
}

/// Debit asset, credit revenue, 100 cents, with one tag on the debit.
fn base_entries() -> Vec<TestEntry> {
    vec![
        (ASSET, Side::Debit, 100, vec![TestTag::Alpha(1)]),
        (REVENUE, Side::Credit, 100, vec![]),
    ]
}

fn base() -> UnpostedTransaction<Usd, TestTag> {
    build("renewal-1", "Renewal", effective(), &base_entries())
}

async fn row_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM transactions"#)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Saves `first`, then `second`, and asserts the second is a conflict with the first.
async fn assert_conflict(
    pool: &PgPool,
    first: UnpostedTransaction<Usd, TestTag>,
    second: UnpostedTransaction<Usd, TestTag>,
) {
    let store = store(pool);
    let first_id = store.save(&first).await.unwrap().transaction().id();
    match store.save(&second).await {
        Err(SaveError::Conflict { key, existing }) => {
            assert_eq!(key, *first.idempotency_key());
            assert_eq!(existing, first_id);
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert_eq!(row_count(pool).await, 1);
}

#[sqlx::test]
async fn saves_and_loads_a_transaction(pool: PgPool) {
    let store = store(&pool);
    let saved = store.save(&base()).await.unwrap();
    let Saved::Recorded(posted) = saved else {
        panic!("expected Recorded, got {saved:?}");
    };
    assert_eq!(posted.recorded_at(), clock_time());
    assert_eq!(store.load(posted.id()).await.unwrap(), Some(posted));
}

#[sqlx::test]
async fn retry_with_same_contents_is_already_recorded(pool: PgPool) {
    let store = store(&pool);
    let first = store.save(&base()).await.unwrap().into_transaction();
    let second = store.save(&base()).await.unwrap();
    assert_eq!(second, Saved::AlreadyRecorded(first));
    assert_eq!(row_count(&pool).await, 1);
}

#[sqlx::test]
async fn retry_with_entries_reordered_is_already_recorded(pool: PgPool) {
    let store = store(&pool);
    let first = store.save(&base()).await.unwrap().into_transaction();
    let mut reordered = base_entries();
    reordered.reverse();
    let retry = build("renewal-1", "Renewal", effective(), &reordered);
    let second = store.save(&retry).await.unwrap();
    assert_eq!(second, Saved::AlreadyRecorded(first));
    assert_eq!(row_count(&pool).await, 1);
}

#[sqlx::test]
async fn same_key_different_description_is_a_conflict(pool: PgPool) {
    let other = build("renewal-1", "Other", effective(), &base_entries());
    assert_conflict(&pool, base(), other).await;
}

#[sqlx::test]
async fn description_whitespace_difference_is_a_conflict(pool: PgPool) {
    let other = build("renewal-1", "Renewal ", effective(), &base_entries());
    assert_conflict(&pool, base(), other).await;
}

#[sqlx::test]
async fn same_key_different_effective_time_is_a_conflict(pool: PgPool) {
    let other = build(
        "renewal-1",
        "Renewal",
        effective() - TimeDelta::hours(1),
        &base_entries(),
    );
    assert_conflict(&pool, base(), other).await;
}

#[sqlx::test]
async fn same_key_different_account_is_a_conflict(pool: PgPool) {
    let mut entries = base_entries();
    entries[1].0 = LIABILITY;
    let other = build("renewal-1", "Renewal", effective(), &entries);
    assert_conflict(&pool, base(), other).await;
}

#[sqlx::test]
async fn same_key_different_side_is_a_conflict(pool: PgPool) {
    let mut entries = base_entries();
    entries[0].1 = Side::Credit;
    entries[1].1 = Side::Debit;
    let other = build("renewal-1", "Renewal", effective(), &entries);
    assert_conflict(&pool, base(), other).await;
}

#[sqlx::test]
async fn same_key_different_amount_is_a_conflict(pool: PgPool) {
    let mut entries = base_entries();
    entries[0].2 = 200;
    entries[1].2 = 200;
    let other = build("renewal-1", "Renewal", effective(), &entries);
    assert_conflict(&pool, base(), other).await;
}

#[sqlx::test]
async fn same_key_different_tags_is_a_conflict(pool: PgPool) {
    let mut entries = base_entries();
    entries[0].3 = vec![TestTag::Alpha(2)];
    let other = build("renewal-1", "Renewal", effective(), &entries);
    assert_conflict(&pool, base(), other).await;
}

#[sqlx::test]
async fn tags_given_out_of_order_round_trip(pool: PgPool) {
    let store = store(&pool);
    let entries = |tags: Vec<TestTag>| {
        vec![
            (ASSET, Side::Debit, 100, tags),
            (REVENUE, Side::Credit, 100, vec![]),
        ]
    };
    let first = build(
        "tags-1",
        "Tags",
        effective(),
        &entries(vec![TestTag::Beta(1), TestTag::Alpha(2)]),
    );
    let Saved::Recorded(posted) = store.save(&first).await.unwrap() else {
        panic!("expected Recorded");
    };
    assert_eq!(store.load(posted.id()).await.unwrap(), Some(posted.clone()));

    let reversed = build(
        "tags-1",
        "Tags",
        effective(),
        &entries(vec![TestTag::Alpha(2), TestTag::Beta(1)]),
    );
    assert_eq!(
        store.save(&reversed).await.unwrap(),
        Saved::AlreadyRecorded(posted)
    );
}

#[sqlx::test]
async fn stores_an_amount_of_i64_max(pool: PgPool) {
    let store = store(&pool);
    let entries = vec![
        (ASSET, Side::Debit, i64::MAX, vec![]),
        (REVENUE, Side::Credit, i64::MAX, vec![]),
    ];
    let big = build("big", "Big", effective(), &entries);
    let posted = store.save(&big).await.unwrap().into_transaction();
    assert_eq!(posted.entries()[0].amount().minor_units(), i64::MAX);
    assert_eq!(store.load(posted.id()).await.unwrap(), Some(posted));
}

#[sqlx::test]
async fn rejects_effective_time_finer_than_a_microsecond(pool: PgPool) {
    let store = store(&pool);
    let precise = effective() + TimeDelta::nanoseconds(1);
    let transaction = build("precise", "Precise", precise, &base_entries());
    match store.save(&transaction).await {
        Err(SaveError::TooPrecise { effective_at }) => assert_eq!(effective_at, precise),
        other => panic!("expected TooPrecise, got {other:?}"),
    }
    assert_eq!(row_count(&pool).await, 0);
}

#[sqlx::test]
async fn recorded_time_is_truncated_to_microseconds(pool: PgPool) {
    let now = clock_time() + TimeDelta::nanoseconds(123_456_789);
    let store = store_at(&pool, chart(), now);
    let posted = store.save(&base()).await.unwrap().into_transaction();
    let expected = clock_time() + TimeDelta::microseconds(123_456);
    assert_eq!(posted.recorded_at(), expected);
    assert_eq!(store.load(posted.id()).await.unwrap(), Some(posted));
}

#[sqlx::test]
async fn effective_after_recorded_is_rejected(pool: PgPool) {
    // The store's clock is behind the transaction's effective time.
    let store = store_at(&pool, chart(), clock_time() - TimeDelta::days(2));
    match store.save(&base()).await {
        Err(SaveError::Rejected(PostError::EffectiveAfterRecorded { .. })) => {}
        other => panic!("expected EffectiveAfterRecorded, got {other:?}"),
    }
    assert_eq!(row_count(&pool).await, 0);
}

#[sqlx::test]
async fn account_missing_from_the_stores_chart_is_rejected(pool: PgPool) {
    let larger = Chart::new(chart().accounts().cloned().chain([Account::new(
        NOT_IN_CHART,
        "Extra",
        AccountType::Asset,
    )]))
    .unwrap();
    let entries = vec![
        (NOT_IN_CHART, Side::Debit, 100, vec![]),
        (REVENUE, Side::Credit, 100, vec![]),
    ];
    let transaction = build_in(&larger, "extra", "Extra", effective(), &entries);
    match store(&pool).save(&transaction).await {
        Err(SaveError::Rejected(PostError::UnknownAccount { account, .. })) => {
            assert_eq!(account, NOT_IN_CHART);
        }
        other => panic!("expected UnknownAccount, got {other:?}"),
    }
    assert_eq!(row_count(&pool).await, 0);
}

#[sqlx::test]
async fn load_of_unknown_id_is_none(pool: PgPool) {
    let store = store(&pool);
    for id in [999, u64::MAX] {
        assert_eq!(store.load(TransactionId::new(id)).await.unwrap(), None);
    }
}

#[sqlx::test]
async fn ids_come_from_the_sequence(pool: PgPool) {
    let store = store(&pool);
    let first = store.save(&base()).await.unwrap().into_transaction();
    let other = build("renewal-2", "Renewal", effective(), &base_entries());
    let second = store.save(&other).await.unwrap().into_transaction();
    assert!(first.id() < second.id());
}

/// A tag carrying free text, to put awkward values through the database.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum TextTag {
    Note(String),
}

impl Tag for TextTag {
    fn kind(&self) -> &'static str {
        "note"
    }
}

impl StoredTag for TextTag {
    fn value(&self) -> String {
        let Self::Note(text) = self;
        text.clone()
    }

    fn from_stored(kind: &str, value: &str) -> Option<Self> {
        (kind == "note").then(|| Self::Note(value.to_string()))
    }
}

/// Debit asset (tagged with the first note, if any), credit revenue (tagged
/// with the second): 100 cents. An entry holds one tag per kind.
fn text_tagged(
    key_text: &str,
    description: &str,
    effective_at: DateTime<Utc>,
    notes: &[&str],
) -> UnpostedTransaction<Usd, TextTag> {
    let tag = |i: usize| {
        notes
            .get(i)
            .map(|n| TextTag::Note((*n).to_string()))
            .into_iter()
            .collect::<Vec<_>>()
    };
    UnpostedTransaction::builder(key(key_text), effective_at, description)
        .debit(ASSET, usd(100), tag(0))
        .credit(REVENUE, usd(100), tag(1))
        .build(&chart(), clock_time())
        .unwrap()
}

fn text_store(pool: &PgPool) -> Store<TextTag> {
    Store::new(
        pool.clone(),
        chart(),
        Arc::new(FixedClock::new(clock_time())),
    )
}

/// Asserts the save is refused as unstorable (naming `what`), is not
/// retryable, and wrote nothing.
async fn assert_unstorable(
    pool: &PgPool,
    transaction: &UnpostedTransaction<Usd, TextTag>,
    what: &str,
) {
    match text_store(pool).save(transaction).await {
        Err(error @ SaveError::Unstorable { .. }) => {
            assert!(!error.is_retryable());
            assert!(error.to_string().contains(what), "{error}");
        }
        other => panic!("expected Unstorable, got {other:?}"),
    }
    assert_eq!(row_count(pool).await, 0);
}

#[sqlx::test]
async fn description_with_nul_is_unstorable_and_not_retryable(pool: PgPool) {
    let transaction = text_tagged("nul-1", "bad\0text", effective(), &[]);
    assert_unstorable(&pool, &transaction, "description contains a NUL byte").await;
}

#[sqlx::test]
async fn tag_value_with_nul_is_unstorable(pool: PgPool) {
    let transaction = text_tagged("nul-2", "Fine", effective(), &["ok", "a\0b"]);
    assert_unstorable(&pool, &transaction, "tag value contains a NUL byte").await;
}

#[sqlx::test]
async fn effective_time_before_4713_bc_is_unstorable(pool: PgPool) {
    // 4713 BC is year -4712 in chrono's astronomical numbering.
    let earliest = Utc.with_ymd_and_hms(-4712, 1, 1, 0, 0, 0).unwrap();
    let too_early = earliest - TimeDelta::seconds(1);
    let transaction = text_tagged("early-1", "Ancient", too_early, &[]);
    assert_unstorable(&pool, &transaction, "effective time is before 4713 BC").await;

    // The boundary itself is storable and round-trips.
    let at_bound = text_tagged("early-2", "Boundary", earliest, &[]);
    let store = text_store(&pool);
    let posted = store.save(&at_bound).await.unwrap().into_transaction();
    assert_eq!(store.load(posted.id()).await.unwrap(), Some(posted));
}

#[sqlx::test]
async fn database_data_exceptions_are_unstorable_not_unavailable(pool: PgPool) {
    // PostgreSQL text can't hold NUL: SQLSTATE 22021, class 22.
    let error = sqlx::query("SELECT $1::text")
        .bind("a\0b")
        .fetch_one(&pool)
        .await
        .unwrap_err();
    assert!(matches!(
        SaveError::from_database(error),
        SaveError::Unstorable { .. }
    ));

    // Anything else stays retryable.
    assert!(SaveError::from_database(sqlx::Error::PoolTimedOut).is_retryable());
}

#[sqlx::test]
async fn text_with_separators_and_case_round_trip_through_the_database(pool: PgPool) {
    let store = text_store(&pool);
    let transaction = text_tagged(
        "text-1",
        "Colons: and \u{e9}",
        effective(),
        &["a:b", "\u{e9}"],
    );
    let posted = store.save(&transaction).await.unwrap().into_transaction();
    let loaded = store.load(posted.id()).await.unwrap().unwrap();
    assert_eq!(loaded, posted);
    assert_eq!(loaded.description(), "Colons: and \u{e9}");
}

#[sqlx::test]
async fn keys_differing_only_in_case_are_different_transactions(pool: PgPool) {
    let store = store(&pool);
    for key_text in ["charge-1", "Charge-1"] {
        let transaction = build(key_text, "Charge", effective(), &base_entries());
        assert!(matches!(
            store.save(&transaction).await.unwrap(),
            Saved::Recorded(_)
        ));
    }
    assert_eq!(row_count(&pool).await, 2);
}

#[test]
fn unavailable_is_the_only_retryable_error() {
    let unavailable = SaveError::Unavailable(sqlx::Error::PoolTimedOut);
    assert!(unavailable.is_retryable());
    assert!(std::error::Error::source(&unavailable).is_some());
    let too_precise = SaveError::TooPrecise {
        effective_at: effective(),
    };
    assert!(!too_precise.is_retryable());
    assert!(std::error::Error::source(&too_precise).is_none());
}

#[sqlx::test]
async fn any_valid_transaction_round_trips_and_retries_as_already_recorded(pool: PgPool) {
    let store = store(&pool);
    let mut runner = TestRunner::default();
    let strategy = valid_entries();
    for case in 0..16 {
        let entries = strategy.new_tree(&mut runner).unwrap().current();
        let transaction = build(&format!("case-{case}"), "Generated", effective(), &entries);
        let posted = store.save(&transaction).await.unwrap().into_transaction();
        assert_eq!(store.load(posted.id()).await.unwrap(), Some(posted.clone()));
        let again = store.save(&transaction).await.unwrap();
        assert_eq!(again, Saved::AlreadyRecorded(posted));
    }
}

// --- Reversals ---

/// Saves `base()` and loads it back: the original the reversal tests reverse.
async fn saved_original(
    store: &Store<TestTag>,
) -> crate::transaction::PostedTransaction<Usd, TestTag> {
    let saved = store.save(&base()).await.unwrap().into_transaction();
    store.load(saved.id()).await.unwrap().unwrap()
}

fn reversal_of(
    original: &crate::transaction::PostedTransaction<Usd, TestTag>,
    key_text: &str,
) -> UnpostedTransaction<Usd, TestTag> {
    original
        .reversal(
            key(key_text),
            effective(),
            "Reverse",
            &chart(),
            clock_time(),
        )
        .unwrap()
}

#[sqlx::test]
async fn saves_a_valid_reversal(pool: PgPool) {
    let store = store(&pool);
    let t1 = saved_original(&store).await;
    let saved = store.save(&reversal_of(&t1, "reverse-1")).await.unwrap();
    let Saved::Recorded(posted) = saved else {
        panic!("expected Recorded, got {saved:?}");
    };
    assert_eq!(posted.reverses(), Some(t1.id()));
    assert_eq!(store.load(posted.id()).await.unwrap(), Some(posted));
}

#[sqlx::test]
async fn reversal_of_missing_original_is_rejected(pool: PgPool) {
    let store = store(&pool);
    let t1 = saved_original(&store).await;
    // Build against a transaction with an ID the ledger never assigned.
    let phantom = build("phantom", "Phantom", effective(), &base_entries())
        .post(&chart(), TransactionId::new(999), clock_time())
        .unwrap();
    let reversal = reversal_of(&phantom, "reverse-missing");
    match store.save(&reversal).await {
        Err(error @ SaveError::OriginalNotFound { .. }) => {
            assert!(!error.is_retryable());
            let SaveError::OriginalNotFound { original } = error else {
                unreachable!()
            };
            assert_eq!(original, TransactionId::new(999));
        }
        other => panic!("expected OriginalNotFound, got {other:?}"),
    }
    assert_ne!(t1.id(), TransactionId::new(999));
    assert_eq!(row_count(&pool).await, 1);
}

#[sqlx::test]
async fn reversal_of_id_beyond_bigint_is_original_not_found(pool: PgPool) {
    let store = store(&pool);
    let phantom = build("phantom", "Phantom", effective(), &base_entries())
        .post(&chart(), TransactionId::new(u64::MAX), clock_time())
        .unwrap();
    let reversal = reversal_of(&phantom, "reverse-huge");
    match store.save(&reversal).await {
        Err(SaveError::OriginalNotFound { original }) => {
            assert_eq!(original, TransactionId::new(u64::MAX));
        }
        other => panic!("expected OriginalNotFound, got {other:?}"),
    }
    assert_eq!(row_count(&pool).await, 0);
}

#[sqlx::test]
async fn reversal_that_doesnt_mirror_is_rejected(pool: PgPool) {
    let store = store(&pool);
    let t1 = saved_original(&store).await;
    // Same accounts and sides flipped, but 200 cents instead of 100.
    let not_a_mirror = UnpostedTransaction::builder(key("bad-reverse"), effective(), "Bad")
        .reversing(t1.id())
        .credit(ASSET, usd(200), [TestTag::Alpha(1)])
        .debit(REVENUE, usd(200), [])
        .build(&chart(), clock_time())
        .unwrap();
    match store.save(&not_a_mirror).await {
        Err(SaveError::Reversal(ReversalError::UnmatchedOriginalEntry { entry })) => {
            assert_eq!(entry, 0);
        }
        other => panic!("expected UnmatchedOriginalEntry, got {other:?}"),
    }
    assert_eq!(row_count(&pool).await, 1);
}

#[sqlx::test]
async fn reversal_effective_before_original_is_rejected(pool: PgPool) {
    let store = store(&pool);
    let t1 = saved_original(&store).await;
    let early = t1.effective_at() - TimeDelta::seconds(1);
    let reversal = UnpostedTransaction::builder(key("early-reverse"), early, "Early")
        .reversing(t1.id())
        .credit(ASSET, usd(100), [TestTag::Alpha(1)])
        .debit(REVENUE, usd(100), [])
        .build(&chart(), clock_time())
        .unwrap();
    match store.save(&reversal).await {
        Err(SaveError::Reversal(ReversalError::EffectiveBeforeOriginal {
            effective_at,
            original_effective_at,
        })) => {
            assert_eq!(effective_at, early);
            assert_eq!(original_effective_at, t1.effective_at());
        }
        other => panic!("expected EffectiveBeforeOriginal, got {other:?}"),
    }
    assert_eq!(row_count(&pool).await, 1);
}

#[sqlx::test]
async fn retry_of_a_reversal_is_already_recorded(pool: PgPool) {
    let store = store(&pool);
    let t1 = saved_original(&store).await;
    let reversal = reversal_of(&t1, "reverse-1");
    let first = store.save(&reversal).await.unwrap().into_transaction();
    let second = store.save(&reversal).await.unwrap();
    assert_eq!(second, Saved::AlreadyRecorded(first));
    assert_eq!(row_count(&pool).await, 2);
}

#[sqlx::test]
async fn two_reversals_of_one_transaction_both_save(pool: PgPool) {
    // "A transaction is reversed at most once" is SUM-10's job (the database
    // enforces it). Until then both save; SUM-10 flips this test.
    let store = store(&pool);
    let t1 = saved_original(&store).await;
    store.save(&reversal_of(&t1, "reverse-1")).await.unwrap();
    store.save(&reversal_of(&t1, "reverse-2")).await.unwrap();
    assert_eq!(row_count(&pool).await, 3);
}

#[sqlx::test]
async fn corrupt_stored_row_is_reported(pool: PgPool) {
    // A balanced transaction whose tag kind the tag type doesn't know. The
    // deferred balance check passes, so the database accepts it.
    let mut db = pool.begin().await.unwrap();
    let id: i64 = sqlx::query_scalar("SELECT nextval('transaction_ids')")
        .fetch_one(&mut *db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO transactions (id, idempotency_key, description, effective_at, recorded_at) \
         VALUES ($1, 'corrupt-1', 'Corrupt', $2, $3)",
    )
    .bind(id)
    .bind(effective())
    .bind(clock_time())
    .execute(&mut *db)
    .await
    .unwrap();
    for (position, account, side) in [(0_i16, 1_i64, "debit"), (1, 3, "credit")] {
        sqlx::query(
            "INSERT INTO entries (transaction_id, position, account, side, amount, currency) \
             VALUES ($1, $2, $3, $4, 100, 'USD')",
        )
        .bind(id)
        .bind(position)
        .bind(account)
        .bind(side)
        .execute(&mut *db)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO entry_tags (transaction_id, position, kind, value) \
         VALUES ($1, 0, 'unknown_kind', '7')",
    )
    .bind(id)
    .execute(&mut *db)
    .await
    .unwrap();
    db.commit().await.unwrap();

    let store = store(&pool);
    match store
        .load(TransactionId::new(u64::try_from(id).unwrap()))
        .await
    {
        Err(LoadError::Corrupt(CorruptData::UnknownTag {
            transaction,
            kind,
            value,
        })) => {
            assert_eq!(transaction, id);
            assert_eq!(kind, "unknown_kind");
            assert_eq!(value, "7");
        }
        other => panic!("expected Corrupt(UnknownTag), got {other:?}"),
    }
}

/// Counts the backends in this database that are waiting on a lock.
async fn lock_waiters(pool: &PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM pg_stat_activity \
         WHERE datname = current_database() AND wait_event_type = 'Lock'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Waits until some save is blocked on a lock. Polling is bounded, not timed:
/// it ends as soon as the interleaving has happened.
async fn wait_for_lock_waiter(pool: &PgPool) {
    let polling = async {
        while lock_waiters(pool).await < 1 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), polling)
        .await
        .expect("save B never blocked on save A's uncommitted insert");
}

/// Save A inserts `base()` and stays open; save B of the same transaction
/// blocks on it. Returns A's posted transaction and B's result, after `finish`
/// has committed or rolled back A.
async fn race(
    pool: &PgPool,
    finish: impl AsyncFnOnce(sqlx::Transaction<'static, sqlx::Postgres>),
) -> (
    crate::transaction::PostedTransaction<Usd, TestTag>,
    Result<Saved<TestTag>, SaveError>,
) {
    let x = base();
    let store_a = store(pool);
    let mut a = pool.begin().await.unwrap();
    let Attempt::Inserted(posted_a) = store_a.insert_on(&mut a, &x, clock_time()).await.unwrap()
    else {
        panic!("save A should have inserted");
    };

    let store_b = store(pool);
    let b = tokio::spawn(async move { store_b.save(&x).await });
    wait_for_lock_waiter(pool).await;
    finish(a).await;
    (posted_a, b.await.unwrap())
}

async fn entry_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM entries"#)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn tag_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM entry_tags"#)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn concurrent_identical_saves_record_once(pool: PgPool) {
    let (posted_a, b) = race(&pool, async |a| a.commit().await.unwrap()).await;
    match b {
        Ok(Saved::AlreadyRecorded(existing)) => assert_eq!(existing.id(), posted_a.id()),
        other => panic!("expected AlreadyRecorded, got {other:?}"),
    }
    assert_eq!(row_count(&pool).await, 1);
    assert_eq!(entry_count(&pool).await, 2);
}

#[sqlx::test]
async fn concurrent_save_after_rollback_records_it(pool: PgPool) {
    let (_, b) = race(&pool, async |a| a.rollback().await.unwrap()).await;
    match b {
        Ok(Saved::Recorded(_)) => {}
        other => panic!("expected Recorded, got {other:?}"),
    }
    assert_eq!(row_count(&pool).await, 1);
    assert_eq!(entry_count(&pool).await, 2);
}

#[sqlx::test]
async fn failure_partway_through_leaves_nothing(pool: PgPool) {
    sqlx::query(
        "CREATE FUNCTION fail_on_second_entry() RETURNS trigger AS $$ \
         BEGIN \
           IF NEW.position = 1 THEN RAISE EXCEPTION 'test failure'; END IF; \
           RETURN NEW; \
         END $$ LANGUAGE plpgsql",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_on_second_entry BEFORE INSERT ON entries \
         FOR EACH ROW EXECUTE FUNCTION fail_on_second_entry()",
    )
    .execute(&pool)
    .await
    .unwrap();

    let store = store(&pool);
    let entries = vec![
        (ASSET, Side::Debit, 100, vec![TestTag::Alpha(1)]),
        (REVENUE, Side::Credit, 100, vec![TestTag::Alpha(2)]),
    ];
    let failing = build("fails", "Fails", effective(), &entries);
    assert!(matches!(
        store.save(&failing).await,
        Err(SaveError::Unavailable(_))
    ));
    assert_eq!(row_count(&pool).await, 0, "transactions");
    assert_eq!(entry_count(&pool).await, 0, "entries");
    assert_eq!(tag_count(&pool).await, 0, "entry_tags");

    // Nothing is left over: with the fault removed, a different transaction saves.
    sqlx::query("DROP TRIGGER fail_on_second_entry ON entries")
        .execute(&pool)
        .await
        .unwrap();
    let other = build("works", "Works", effective(), &base_entries());
    assert!(matches!(store.save(&other).await, Ok(Saved::Recorded(_))));
    assert_eq!(row_count(&pool).await, 1);
}
