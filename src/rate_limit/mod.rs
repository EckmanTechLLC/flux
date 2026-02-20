// Rate limiting for event ingestion (ADR-006 Session 3)
//
// Per-namespace token bucket. Active only when auth_enabled=true.
// Limit is read from SharedRuntimeConfig on each check, so admin API changes
// take effect immediately for new refill calculations.

use dashmap::DashMap;
use std::time::Instant;

/// Token bucket for a single namespace.
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: u64) -> Self {
        Self {
            tokens: capacity as f64,
            last_refill: Instant::now(),
        }
    }

    /// Try to consume one token. Refills based on elapsed time at rate = capacity/60 tokens/sec.
    fn try_consume(&mut self, capacity: u64) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let refill_rate = capacity as f64 / 60.0;
        self.tokens = (self.tokens + elapsed * refill_rate).min(capacity as f64);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Per-namespace token bucket rate limiter.
///
/// Buckets are created lazily on first event. State is in-memory only (resets on restart).
pub struct RateLimiter {
    buckets: DashMap<String, TokenBucket>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            buckets: DashMap::new(),
        }
    }

    /// Check and consume one token for `namespace` at `limit_per_minute`.
    ///
    /// Returns true if the request is allowed, false if rate limit exceeded.
    pub fn check_and_consume(&self, namespace: &str, limit_per_minute: u64) -> bool {
        let mut bucket = self
            .buckets
            .entry(namespace.to_string())
            .or_insert_with(|| TokenBucket::new(limit_per_minute));
        bucket.try_consume(limit_per_minute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_within_limit() {
        let limiter = RateLimiter::new();
        // Bucket starts full — first request must be allowed
        assert!(limiter.check_and_consume("ns1", 100));
    }

    #[test]
    fn test_blocks_when_bucket_empty() {
        let limiter = RateLimiter::new();
        // Drain the bucket (capacity = 1)
        assert!(limiter.check_and_consume("ns1", 1));
        // Next immediate request must be blocked
        assert!(!limiter.check_and_consume("ns1", 1));
    }

    #[test]
    fn test_separate_buckets_per_namespace() {
        let limiter = RateLimiter::new();
        // Drain ns1
        assert!(limiter.check_and_consume("ns1", 1));
        assert!(!limiter.check_and_consume("ns1", 1));
        // ns2 is unaffected
        assert!(limiter.check_and_consume("ns2", 1));
    }

    #[test]
    fn test_refill_over_time() {
        let limiter = RateLimiter::new();
        // Drain a bucket with 1 token
        assert!(limiter.check_and_consume("ns1", 1));
        assert!(!limiter.check_and_consume("ns1", 1));

        // Manually advance time by manipulating: we can't freeze Instant, so instead
        // set capacity=60 and drain quickly — then wait 1ms (refill = 60/60/1000 = 0.001/ms)
        // This test just verifies the refill path compiles and runs without panic.
        std::thread::sleep(std::time::Duration::from_millis(70));
        // After 70ms at 1 token/minute: refill ≈ 60/60 * 0.07 = 0.07 tokens — not enough
        // With capacity=3600 tokens/minute (60/sec): 0.07 sec * 60 = 4.2 tokens refilled
        let limiter2 = RateLimiter::new();
        assert!(limiter2.check_and_consume("ns1", 3600)); // fresh bucket, allowed
        // drain it
        for _ in 0..3599 {
            limiter2.check_and_consume("ns1", 3600);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        // 20ms at 60 tokens/sec = 1.2 tokens refilled → next should be allowed
        assert!(limiter2.check_and_consume("ns1", 3600));
    }
}
