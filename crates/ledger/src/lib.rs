//! The ledger core: an append-only, double-entry ledger that every other
//! module in Summa is built on.

mod account;
mod balance;
mod chart;
mod clock;
mod idempotency;
mod money;
mod reversal;
#[cfg(feature = "postgres")]
mod store;
mod tag;
#[cfg(test)]
mod test_support;
mod transaction;

pub use account::{Account, AccountNumber, AccountType, Permanence, Side};
pub use balance::{
    BalanceError, BalanceQuery, Cutoffs, TrialBalance, TrialBalanceLine, balance, trial_balance,
};
pub use chart::{Chart, ChartError};
#[cfg(feature = "postgres")]
pub use clock::SystemClock;
pub use clock::{Clock, FixedClock};
pub use idempotency::{IdempotencyKey, IdempotencyKeyError};
pub use money::{Currency, Money, MoneyError, Usd};
pub use reversal::ReversalError;
#[cfg(feature = "postgres")]
pub use store::{CorruptData, StoredTag};
pub use tag::Tag;
pub use transaction::{
    Entry, PostError, PostedTransaction, TransactionBuilder, TransactionError, TransactionId,
    UnpostedTransaction,
};
