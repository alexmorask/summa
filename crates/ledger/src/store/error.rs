//! What can go wrong saving or loading a transaction.

use std::error::Error;
use std::fmt;

use chrono::{DateTime, Utc};

use super::rows::CorruptData;
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
    /// The transaction reverses one that isn't in the ledger.
    OriginalNotFound { original: TransactionId },
    /// The transaction isn't a valid reversal of the one it names.
    Reversal(ReversalError<Usd>),
    /// The database couldn't be reached or failed. The transaction may or may
    /// not have been saved (a commit can succeed on the server and the reply
    /// be lost). Retrying with the same key is safe.
    Unavailable(sqlx::Error),
    /// Stored data couldn't be read back.
    Corrupt(CorruptData),
}

impl SaveError {
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
            SaveError::OriginalNotFound { original } => write!(
                f,
                "the transaction it reverses ({}) is not in the ledger",
                original.get()
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
