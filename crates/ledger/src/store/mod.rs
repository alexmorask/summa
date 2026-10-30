//! Storage of the ledger in PostgreSQL.

mod error;
mod load;
mod rows;
mod save;
#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod store_tests;
mod stored_tag;

use std::sync::Arc;

use sqlx::PgPool;

use crate::chart::Chart;
use crate::clock::Clock;
use crate::money::Usd;
use crate::transaction::PostedTransaction;

pub use error::{LoadError, SaveError};
pub use rows::CorruptData;
pub use stored_tag::StoredTag;

/// The ledger's PostgreSQL store. It owns the chart posting checks against
/// and the clock that stamps recorded times; callers supply neither per call.
pub struct Store<T: StoredTag> {
    pool: PgPool,
    chart: Chart,
    clock: Arc<dyn Clock>,
    // `T` appears only in method signatures; this ties it to the type.
    tags: std::marker::PhantomData<fn() -> T>,
}

impl<T: StoredTag> Store<T> {
    /// A store over `pool`, checking postings against `chart` and stamping
    /// them with `clock`'s time.
    pub fn new(pool: PgPool, chart: Chart, clock: Arc<dyn Clock>) -> Self {
        Self {
            pool,
            chart,
            clock,
            tags: std::marker::PhantomData,
        }
    }
}

/// What a successful `save` did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Saved<T: StoredTag> {
    /// The transaction was new and is now recorded.
    Recorded(PostedTransaction<Usd, T>),
    /// The same request was recorded earlier; this is that transaction and
    /// nothing was written.
    AlreadyRecorded(PostedTransaction<Usd, T>),
}

impl<T: StoredTag> Saved<T> {
    /// The recorded transaction, whether new or not.
    pub fn transaction(&self) -> &PostedTransaction<Usd, T> {
        match self {
            Saved::Recorded(posted) | Saved::AlreadyRecorded(posted) => posted,
        }
    }

    /// The recorded transaction, by value.
    pub fn into_transaction(self) -> PostedTransaction<Usd, T> {
        match self {
            Saved::Recorded(posted) | Saved::AlreadyRecorded(posted) => posted,
        }
    }
}
