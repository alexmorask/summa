//! The clock: where "now" comes from. The ledger never reads the system clock
//! itself; the app's edge hands it a `Clock` (ADR 0003).

#[cfg(feature = "postgres")]
use chrono::SubsecRound;
use chrono::{DateTime, Utc};

/// A source of the current time.
///
/// The current time is always UTC, and it must never go backwards: two calls
/// on the same clock return times in order. The app's edge chooses the
/// implementation: the real clock in production, a fixed one in tests and a
/// simulated one in the demo (ADR 0003). The ledger never reads the system
/// clock itself.
///
/// `Send + Sync` because the clock lives in an `Arc<dyn Clock>` shared across
/// async tasks.
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use ledger::{Clock, FixedClock};
///
/// let t = Utc.with_ymd_and_hms(2026, 10, 29, 12, 0, 0).unwrap();
/// let clock = FixedClock::new(t);
/// assert_eq!(clock.now(), t);
/// ```
pub trait Clock: Send + Sync {
    /// The current time, in UTC.
    ///
    /// An implementation that feeds the store returns whole microseconds. The
    /// store truncates `recorded_at` itself, but it rejects an `effective_at`
    /// finer than a microsecond (the database can't keep it), so a clock that
    /// supplies effective times must not return nanoseconds.
    fn now(&self) -> DateTime<Utc>;
}

/// A clock that always returns the same time. Use it in tests, and to pin
/// "now" for a single run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedClock(DateTime<Utc>);

impl FixedClock {
    /// A clock that always returns `at`.
    pub fn new(at: DateTime<Utc>) -> Self {
        Self(at)
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

/// The real clock: the system's time in UTC, truncated to whole microseconds. The only place in the ledger
/// that reads the system clock (ADR 0003).
#[cfg(feature = "postgres")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SystemClock;

#[cfg(feature = "postgres")]
impl Clock for SystemClock {
    #[allow(clippy::disallowed_methods, reason = "the one real clock (ADR 0003)")]
    fn now(&self) -> DateTime<Utc> {
        // Whole microseconds: Linux reports nanoseconds, which the store
        // would reject as an effective time (see `Clock::now`).
        Utc::now().trunc_subsecs(6)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::TimeZone;

    use super::*;

    #[test]
    fn fixed_clock_returns_its_time() {
        let t = Utc.with_ymd_and_hms(2026, 10, 29, 12, 0, 0).unwrap();
        let clock = FixedClock::new(t);
        assert_eq!(clock.now(), t);
        assert_eq!(clock.now(), t);
    }

    #[test]
    fn fixed_clock_is_usable_as_a_trait_object() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn Clock>();

        let t = Utc.with_ymd_and_hms(2026, 10, 29, 12, 0, 0).unwrap();
        let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(t));
        assert_eq!(clock.now(), t);

        let shared = Arc::clone(&clock);
        let from_thread = std::thread::spawn(move || shared.now()).join().unwrap();
        assert_eq!(from_thread, t);
    }

    #[cfg(feature = "postgres")]
    #[test]
    fn system_clock_returns_whole_microseconds() {
        for _ in 0..100 {
            assert_eq!(SystemClock.now().timestamp_subsec_nanos() % 1000, 0);
        }
    }

    #[cfg(feature = "postgres")]
    #[test]
    fn system_clock_implements_clock() {
        // Compile-only: a real-time assertion would be flaky.
        fn assert_clock<C: Clock>() {}
        assert_clock::<SystemClock>();
    }
}
