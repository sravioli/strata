//! The limits of one `strata api` process: the token buckets of the status routes (tech design
//! 0047) and the bound of requests at the same time (0002, 0049).
//!
//! The buckets are in the memory of one process, so with N processes an account can read one
//! revision up to N times the limit. The limit protects the API. It is not exact (0047).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use uuid::Uuid;

/// The buckets that one process keeps, at most (0049).
pub const MAX_BUCKETS: usize = 100_000;
/// The bucket of one pair (account, revision): 3 tokens, 1 token for each `Retry-After` (0047).
const REVISION_TOKENS: f64 = 3.0;
/// The bucket of one account: 20 tokens, 10 tokens each second (0047).
const ACCOUNT_TOKENS: f64 = 20.0;
const ACCOUNT_REFILL_PER_SECOND: f64 = 10.0;

/// The time of the buckets. `Clock::system()` in `strata api`. A test moves a manual clock.
#[derive(Clone, Debug)]
pub struct Clock(Option<Arc<Mutex<Instant>>>);

impl Clock {
    pub fn system() -> Clock {
        Clock(None)
    }

    pub fn manual() -> Clock {
        Clock(Some(Arc::new(Mutex::new(Instant::now()))))
    }

    pub fn now(&self) -> Instant {
        match &self.0 {
            None => Instant::now(),
            Some(t) => *t.lock().expect("the clock"),
        }
    }

    /// Moves a manual clock. A system clock does not change.
    pub fn advance(&self, d: Duration) {
        if let Some(t) = &self.0 {
            *t.lock().expect("the clock") += d;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Key {
    Account(u128),
    Revision(u128, Uuid, i32),
}

struct Bucket {
    tokens: f64,
    at: Instant,
    /// The tick of the last use, for the order of removal.
    used: u64,
}

struct Inner {
    buckets: HashMap<Key, Bucket>,
    /// tick of the last use → key. The first entry is the bucket used least recently.
    order: BTreeMap<u64, Key>,
    tick: u64,
}

/// The token buckets of the status routes.
pub struct Buckets {
    inner: Mutex<Inner>,
    /// The `Retry-After` of a status: the refill interval of a revision bucket.
    interval: Duration,
    max: usize,
    pub clock: Clock,
}

impl Buckets {
    pub fn new(retry_after_seconds: u64, max: usize, clock: Clock) -> Buckets {
        Buckets {
            inner: Mutex::new(Inner {
                buckets: HashMap::new(),
                order: BTreeMap::new(),
                tick: 0,
            }),
            interval: Duration::from_secs(retry_after_seconds.max(1)),
            max: max.max(1),
            clock,
        }
    }

    fn capacity(&self, key: Key) -> (f64, f64) {
        match key {
            Key::Account(_) => (ACCOUNT_TOKENS, ACCOUNT_REFILL_PER_SECOND),
            Key::Revision(..) => (REVISION_TOKENS, 1.0 / self.interval.as_secs_f64()),
        }
    }

    /// One status request of `account` for `revisions`: 1 token from the bucket of the account,
    /// and 1 token from the bucket of each revision. When one bucket is empty, the request takes
    /// no token, and the result is the time until that bucket gets its next token.
    pub fn take(&self, account: u128, revisions: &[(Uuid, i32)]) -> Result<(), Duration> {
        let now = self.clock.now();
        let mut keys = vec![Key::Account(account)];
        keys.extend(revisions.iter().map(|&(d, r)| Key::Revision(account, d, r)));
        let mut inner = self.inner.lock().expect("the buckets");
        let mut wait = Duration::ZERO;
        for &key in &keys {
            let (cap, per_second) = self.capacity(key);
            let tokens = inner.refill(key, cap, per_second, now, self.max);
            if tokens < 1.0 {
                wait = wait.max(Duration::from_secs_f64((1.0 - tokens) / per_second));
            }
        }
        if wait > Duration::ZERO {
            return Err(wait);
        }
        for key in keys {
            if let Some(b) = inner.buckets.get_mut(&key) {
                b.tokens -= 1.0;
            }
        }
        Ok(())
    }

    /// The count of buckets, for the tests of the bound.
    pub fn len(&self) -> usize {
        self.inner.lock().expect("the buckets").buckets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Inner {
    /// The tokens of `key` now, after the refill. A new bucket is full. At the bound, the bucket
    /// used least recently goes before a new bucket comes.
    fn refill(&mut self, key: Key, cap: f64, per_second: f64, now: Instant, max: usize) -> f64 {
        self.tick += 1;
        let tick = self.tick;
        if let Some(b) = self.buckets.get_mut(&key) {
            let elapsed = now.saturating_duration_since(b.at).as_secs_f64();
            b.tokens = (b.tokens + elapsed * per_second).min(cap);
            b.at = now;
            self.order.remove(&b.used);
            b.used = tick;
            self.order.insert(tick, key);
            return b.tokens;
        }
        while self.buckets.len() >= max {
            let Some((_, old)) = self.order.pop_first() else {
                break;
            };
            self.buckets.remove(&old);
        }
        self.buckets.insert(
            key,
            Bucket {
                tokens: cap,
                at: now,
                used: tick,
            },
        );
        self.order.insert(tick, key);
        cap
    }
}

/// The requests that one API process handles at the same time, at most (0049).
pub const MAX_REQUESTS: usize = 256;

/// The bound of requests at the same time. A request above it gets `503` with `Retry-After: 1`
/// at once, with no wait (S4 of 0002), and the API logs the rejection.
pub async fn bound(
    axum::extract::State(s): axum::extract::State<crate::api::AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let Ok(_permit) = s.in_flight.clone().try_acquire_owned() else {
        tracing::warn!(bound = "requests", "a request rejected at a bound");
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            [(axum::http::header::RETRY_AFTER, "1")],
        )
            .into_response();
    };
    next.run(req).await
}

/// The `Retry-After` of `429`: the wait until the next token, in whole seconds, at least 1.
pub fn retry_after_seconds(wait: Duration) -> u64 {
    wait.as_secs_f64().ceil().max(1.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rev(n: u128) -> (Uuid, i32) {
        (Uuid::from_u128(n), 1)
    }

    #[test]
    fn a_revision_bucket_holds_3_tokens_and_gets_1_each_interval() {
        let b = Buckets::new(2, MAX_BUCKETS, Clock::manual());
        for _ in 0..3 {
            assert_eq!(b.take(7, &[rev(1)]), Ok(()));
        }
        let wait = b.take(7, &[rev(1)]).unwrap_err();
        assert_eq!(retry_after_seconds(wait), 2);
        // A different revision has its own bucket.
        assert_eq!(b.take(7, &[rev(2)]), Ok(()));
        b.clock.advance(Duration::from_secs(2));
        assert_eq!(b.take(7, &[rev(1)]), Ok(()));
        assert!(b.take(7, &[rev(1)]).is_err());
    }

    #[test]
    fn an_empty_bucket_takes_no_token_from_the_others() {
        let b = Buckets::new(2, MAX_BUCKETS, Clock::manual());
        for _ in 0..3 {
            b.take(7, &[rev(1)]).unwrap();
        }
        assert!(b.take(7, &[rev(1), rev(2)]).is_err());
        // The bucket of revision 2 is still full.
        for _ in 0..3 {
            assert_eq!(b.take(7, &[rev(2)]), Ok(()));
        }
    }

    #[test]
    fn the_account_bucket_holds_20_tokens_and_gets_10_each_second() {
        let b = Buckets::new(2, MAX_BUCKETS, Clock::manual());
        for n in 0..20 {
            assert_eq!(b.take(7, &[rev(n)]), Ok(()), "{n}");
        }
        let wait = b.take(7, &[rev(100)]).unwrap_err();
        assert_eq!(retry_after_seconds(wait), 1);
        assert!(wait <= Duration::from_millis(100));
        b.clock.advance(Duration::from_millis(100));
        assert_eq!(b.take(7, &[rev(100)]), Ok(()));
        // A different account has its own bucket.
        assert_eq!(b.take(8, &[rev(1)]), Ok(()));
    }

    #[test]
    fn the_bucket_used_least_recently_goes_first() {
        // The bound counts the account bucket too: account, R1, R2.
        let b = Buckets::new(2, 3, Clock::manual());
        b.take(7, &[rev(2)]).unwrap();
        b.take(7, &[rev(1)]).unwrap();
        b.take(7, &[rev(1)]).unwrap();
        b.take(7, &[rev(1)]).unwrap();
        assert_eq!(b.len(), 3);
        // R3 comes, and R2, the bucket used least recently, goes.
        b.take(7, &[rev(3)]).unwrap();
        assert_eq!(b.len(), 3);
        // R1 kept its empty bucket. R2 is a full bucket again.
        assert!(b.take(7, &[rev(1)]).is_err());
        for _ in 0..3 {
            assert_eq!(b.take(7, &[rev(2)]), Ok(()));
        }
    }
}
