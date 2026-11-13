//! What can go wrong saving or loading a transaction, or running the balance worker.

use std::error::Error;
use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};

use super::rows::{CorruptData, account_type_text};
use crate::account::{AccountKind, AccountNumber};
use crate::balance::BalanceError;
use crate::idempotency::IdempotencyKey;
use crate::money::Usd;
use crate::reversal::ReversalError;
use crate::transaction::{PostError, TransactionId, utc};

/// Why `save` didn't record a transaction.
#[derive(Debug)]
pub enum SaveError {
    /// The key is already used by a transaction with different contents.
    /// Nothing was saved. `existing` is the transaction that holds the key.
    Conflict {
        key: IdempotencyKey,
        existing: TransactionId,
    },
    /// The transaction failed the ledger's own checks when it was posted.
    Rejected(PostError),
    /// The effective time has finer than microsecond precision, which the
    /// database can't keep. Round-tripping it would change it, so it is
    /// refused.
    TooPrecise { effective_at: DateTime<Utc> },
    /// The database can never store this input, so retrying repeats the
    /// failure. `what` names the part that can't be stored.
    Unstorable { what: &'static str },
    /// The transaction reverses one that isn't in the ledger.
    OriginalNotFound { original: TransactionId },
    /// The transaction it reverses has already been reversed. A transaction is
    /// reversed at most once. Nothing was saved.
    AlreadyReversed { original: TransactionId },
    /// The chart gives an account a different type or contra flag than the
    /// ledger recorded when the account got its first entry. Those two facts
    /// can't change once an account has entries. Nothing was saved.
    AccountChanged {
        account: AccountNumber,
        recorded: AccountKind,
        chart: AccountKind,
    },
    /// The transaction isn't a valid reversal of the one it names.
    Reversal(ReversalError<Usd>),
    /// The database couldn't be reached or failed. The transaction may or may
    /// not have been saved (a commit can succeed on the server and the reply
    /// be lost). Retrying with the same key is safe.
    Unavailable(sqlx::Error),
    /// Stored data couldn't be read back.
    Corrupt(CorruptData),
}

/// The unique constraint that lets a transaction be reversed at most once
/// (named in the migration).
const REVERSES_UNIQUE: &str = "transactions_reverses_unique";

/// Whether `error` is a database error that violated the constraint named
/// `constraint`.
fn violates(error: &sqlx::Error, constraint: &str) -> bool {
    error
        .as_database_error()
        .and_then(|db| db.constraint())
        .is_some_and(|name| name == constraint)
}

/// `liability`, or `contra-revenue` for a contra account.
fn kind_text(kind: AccountKind) -> String {
    let text = account_type_text(kind.account_type());
    if kind.is_contra() {
        format!("contra-{text}")
    } else {
        text.to_owned()
    }
}

impl SaveError {
    /// Maps a database error from the insert path. A data exception
    /// (SQLSTATE class `22`, such as text with a NUL or a timestamp out of
    /// range) will fail the same way every time, so it is `Unstorable`;
    /// anything else is `Unavailable`.
    pub(crate) fn from_database(error: sqlx::Error) -> Self {
        let data_exception = error
            .as_database_error()
            .and_then(|db| db.code())
            .is_some_and(|code| code.starts_with("22"));
        if data_exception {
            SaveError::Unstorable {
                what: "data the database rejected",
            }
        } else {
            SaveError::Unavailable(error)
        }
    }

    /// Maps a database error from the `transactions` insert. A second reversal
    /// of `reverses` violates the unique constraint on that column, which is
    /// [`AlreadyReversed`](Self::AlreadyReversed); anything else is as in
    /// [`from_database`](Self::from_database).
    pub(crate) fn from_transaction_insert(
        error: sqlx::Error,
        reverses: Option<TransactionId>,
    ) -> Self {
        match reverses {
            Some(original) if violates(&error, REVERSES_UNIQUE) => {
                SaveError::AlreadyReversed { original }
            }
            _ => SaveError::from_database(error),
        }
    }

    /// Whether trying again with the same transaction might succeed. True only
    /// for [`Unavailable`](Self::Unavailable); every other error repeats.
    pub fn is_retryable(&self) -> bool {
        matches!(self, SaveError::Unavailable(_))
    }
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Conflict { key, existing } => write!(
                f,
                "idempotency key {:?} is already used by transaction {} with different contents",
                key.as_str(),
                existing.get()
            ),
            SaveError::Rejected(error) => write!(f, "transaction rejected: {error}"),
            SaveError::TooPrecise { effective_at } => write!(
                f,
                "effective time {} is finer than a microsecond, which the ledger can't store",
                utc(*effective_at)
            ),
            SaveError::Unstorable { what } => {
                write!(f, "the ledger can't store this transaction: {what}")
            }
            SaveError::OriginalNotFound { original } => write!(
                f,
                "the transaction it reverses ({}) is not in the ledger",
                original.get()
            ),
            SaveError::AlreadyReversed { original } => {
                write!(f, "transaction {} is already reversed", original.get())
            }
            SaveError::AccountChanged {
                account,
                recorded,
                chart,
            } => write!(
                f,
                "account {} is recorded as {}, but the chart says {}; an account's type and contra flag can't change once it has entries",
                account.get(),
                kind_text(*recorded),
                kind_text(*chart)
            ),
            SaveError::Reversal(error) => write!(f, "invalid reversal: {error}"),
            SaveError::Unavailable(error) => write!(f, "database unavailable: {error}"),
            SaveError::Corrupt(error) => write!(f, "{error}"),
        }
    }
}

impl Error for SaveError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SaveError::Unavailable(error) => Some(error),
            _ => None,
        }
    }
}

/// Why `load` didn't return a transaction.
#[derive(Debug)]
pub enum LoadError {
    /// The database couldn't be reached or failed.
    Unavailable(sqlx::Error),
    /// Stored data couldn't be read back.
    Corrupt(CorruptData),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Unavailable(error) => write!(f, "database unavailable: {error}"),
            LoadError::Corrupt(error) => write!(f, "{error}"),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LoadError::Unavailable(error) => Some(error),
            LoadError::Corrupt(_) => None,
        }
    }
}

impl From<LoadError> for SaveError {
    fn from(error: LoadError) -> Self {
        match error {
            LoadError::Unavailable(error) => SaveError::Unavailable(error),
            LoadError::Corrupt(error) => SaveError::Corrupt(error),
        }
    }
}

/// Why a balance worker batch didn't finish. A failed batch writes nothing:
/// the balances and the checkpoint stay as they were.
#[derive(Debug)]
pub enum WorkerError {
    /// A transient database failure: the connection, I/O or pool failed, or
    /// the database error has SQLSTATE class `08` (connection), `40`
    /// (serialization failure or deadlock), `53` (insufficient resources) or
    /// `57` (operator intervention, which includes shutdown and cancelled
    /// queries such as `57014`, raised by `statement_timeout`), or code
    /// `55P03` (lock not available). Running the batch again is safe and may
    /// succeed.
    Unavailable(sqlx::Error),
    /// The worker stopped because the database refused the batch for a reason that won't go away on its
    /// own: every database error that is not transient, such as a violated
    /// constraint, bad data, a raised exception or a value that can't be
    /// decoded. Running the batch again fails the same way, so the worker
    /// stops. `transactions` is the first and last ledger transaction of the
    /// batch in the order the worker reads them (`(db_tx_id, id)` order), so
    /// the first id can be larger than the last; it is not a numeric range. It
    /// is set for failures while the batch is being processed or written, and
    /// `None` for failures before the batch is read or when committing. (A
    /// commit that fails for a transient reason is
    /// [`Unavailable`](Self::Unavailable), like any other transient failure.)
    Stopped {
        transactions: Option<(TransactionId, TransactionId)>,
        source: sqlx::Error,
    },
    /// Stored entries couldn't be read back. The error names the transaction.
    Corrupt(CorruptData),
    /// Adding `transaction` would take a balance total beyond what an `i64`
    /// holds. Running the batch again fails the same way.
    Overflow { transaction: TransactionId },
}

impl WorkerError {
    /// Classifies a database error from the worker. Transient failures are
    /// [`Unavailable`](Self::Unavailable): an I/O, TLS or pool error, a
    /// crashed driver worker, or a database error with SQLSTATE class `08`
    /// (connection), `40` (serialization failure or deadlock), `53`
    /// (insufficient resources) or `57` (operator intervention), or code
    /// `55P03` (lock not available). Everything else is
    /// [`Stopped`](Self::Stopped), naming `batch` (the first and last ledger
    /// transaction id) when it is known.
    pub(crate) fn from_database(
        error: sqlx::Error,
        batch: Option<(TransactionId, TransactionId)>,
    ) -> Self {
        let transient = match &error {
            sqlx::Error::Io(_)
            | sqlx::Error::Tls(_)
            | sqlx::Error::PoolTimedOut
            | sqlx::Error::PoolClosed
            | sqlx::Error::WorkerCrashed => true,
            sqlx::Error::Database(db) => db.code().is_some_and(|code| {
                ["08", "40", "53", "57"]
                    .iter()
                    .any(|class| code.starts_with(class))
                    || code == "55P03"
            }),
            _ => false,
        };
        if transient {
            WorkerError::Unavailable(error)
        } else {
            WorkerError::Stopped {
                transactions: batch,
                source: error,
            }
        }
    }

    /// Whether running the batch again might succeed. True only for
    /// [`Unavailable`](Self::Unavailable), a transient database failure;
    /// [`Stopped`](Self::Stopped), [`Corrupt`](Self::Corrupt) and
    /// [`Overflow`](Self::Overflow) fail the same way on every run.
    pub fn is_retryable(&self) -> bool {
        matches!(self, WorkerError::Unavailable(_))
    }
}

impl fmt::Display for WorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkerError::Unavailable(error) => write!(f, "database unavailable: {error}"),
            WorkerError::Stopped {
                transactions: Some((first, last)),
                source,
            } => write!(
                f,
                "balance worker stopped at transactions {} to {}: {source}",
                first.get(),
                last.get()
            ),
            WorkerError::Stopped {
                transactions: None,
                source,
            } => write!(f, "balance worker stopped: {source}"),
            WorkerError::Corrupt(error) => write!(f, "{error}"),
            WorkerError::Overflow { transaction } => write!(
                f,
                "balance worker stopped at transaction {}: a balance total would overflow",
                transaction.get()
            ),
        }
    }
}

impl Error for WorkerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            WorkerError::Unavailable(error) | WorkerError::Stopped { source: error, .. } => {
                Some(error)
            }
            WorkerError::Corrupt(_) | WorkerError::Overflow { .. } => None,
        }
    }
}

/// Why `stored_balance` couldn't read a balance.
#[derive(Debug)]
pub enum StoredBalanceError {
    /// The account isn't in the chart, or the balance doesn't fit in the
    /// ledger's integer amounts. The overflow case cannot occur today: the
    /// balance is a subtraction of two non-negative `i64`s.
    Balance(BalanceError),
    /// The database couldn't be reached or failed. Reading again is safe.
    Unavailable(sqlx::Error),
    /// A stored total is negative, which its table forbids.
    Corrupt(CorruptData),
}

impl fmt::Display for StoredBalanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoredBalanceError::Balance(error) => write!(f, "{error}"),
            StoredBalanceError::Unavailable(error) => write!(f, "database unavailable: {error}"),
            StoredBalanceError::Corrupt(error) => write!(f, "{error}"),
        }
    }
}

impl Error for StoredBalanceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StoredBalanceError::Balance(error) => Some(error),
            StoredBalanceError::Unavailable(error) => Some(error),
            StoredBalanceError::Corrupt(error) => Some(error),
        }
    }
}

/// Why a [`BalanceWorker`](super::BalanceWorker) can't be made from a
/// [`WorkerConfig`](super::WorkerConfig).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidWorkerConfig {
    /// The first retry wait is longer than the cap on retry waits, so the cap
    /// would shorten the very first wait.
    RetryWaitAboveCap {
        retry_wait: Duration,
        max_retry_wait: Duration,
    },
}

impl fmt::Display for InvalidWorkerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidWorkerConfig::RetryWaitAboveCap {
                retry_wait,
                max_retry_wait,
            } => write!(
                f,
                "retry wait {retry_wait:?} is longer than the cap on retry waits, {max_retry_wait:?}"
            ),
        }
    }
}

impl Error for InvalidWorkerConfig {}
