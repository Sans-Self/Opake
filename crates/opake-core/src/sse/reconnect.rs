// Exponential backoff policy for SSE reconnection.
//
// Matches the semantics of the shipped TypeScript EventStream:
// - Initial delay: 1s
// - Doubling: 1s → 2s → 4s → 8s → 16s → 30s (capped)
// - Jitter: ±20% (uniform) to avoid thundering herds on server restart
// - `wasConnected` flag: only trigger full-sync on recovery from a
//   previously-open connection, not on initial-connect failures

use std::time::Duration;

const DEFAULT_INITIAL_DELAY_MS: u64 = 1_000;
const DEFAULT_MAX_DELAY_MS: u64 = 30_000;
const JITTER_FRACTION: f64 = 0.20;

/// Exponential backoff state machine. Each call to [`next_delay`] doubles
/// the delay and applies jitter, saturating at `max_delay_ms`.
#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    current_ms: u64,
    max_ms: u64,
}

impl BackoffPolicy {
    pub fn new() -> Self {
        Self {
            current_ms: DEFAULT_INITIAL_DELAY_MS,
            max_ms: DEFAULT_MAX_DELAY_MS,
        }
    }

    pub fn with_max(max_ms: u64) -> Self {
        Self {
            current_ms: DEFAULT_INITIAL_DELAY_MS,
            max_ms,
        }
    }

    /// Reset to the initial delay. Called after a successful connection.
    pub fn reset(&mut self) {
        self.current_ms = DEFAULT_INITIAL_DELAY_MS;
    }

    /// Compute the next reconnect delay, advancing internal state.
    ///
    /// The `rand` parameter supplies the jitter factor in `[0.0, 1.0)`.
    /// Taking it as a parameter (rather than sampling internally) keeps
    /// this pure and testable without pulling `rand` into opake-core.
    pub fn next_delay(&mut self, rand: f64) -> Duration {
        let base = self.current_ms as f64;
        let jitter_range = base * JITTER_FRACTION;
        // Map [0,1) uniformly to [-jitter_range, +jitter_range).
        let offset = (rand * 2.0 - 1.0) * jitter_range;
        let jittered_ms = (base + offset).max(0.0) as u64;

        // Double for next time, clamped.
        self.current_ms = (self.current_ms * 2).min(self.max_ms);

        Duration::from_millis(jittered_ms)
    }
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_delay_near_one_second() {
        let mut policy = BackoffPolicy::new();
        // With rand=0.5 (no offset), first delay should be exactly 1000ms.
        let delay = policy.next_delay(0.5);
        assert_eq!(delay, Duration::from_millis(1000));
    }

    #[test]
    fn jitter_at_bounds_stays_in_range() {
        let mut policy = BackoffPolicy::new();
        // rand=0.0 → -20% → 800ms
        let low = policy.next_delay(0.0);
        assert_eq!(low, Duration::from_millis(800));

        // Reset and try the other bound.
        policy.reset();
        // rand=0.999... → +20% → ~1200ms
        let high = policy.next_delay(0.9999);
        // Approximate check since we use f64 math.
        assert!(high >= Duration::from_millis(1199));
        assert!(high <= Duration::from_millis(1200));
    }

    #[test]
    fn exponential_doubling_up_to_cap() {
        let mut policy = BackoffPolicy::new();
        // Consume successive delays at rand=0.5 (no jitter offset).
        let expected = [
            1_000u64, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000, 30_000,
        ];
        for ms in expected {
            let d = policy.next_delay(0.5);
            assert_eq!(d, Duration::from_millis(ms));
        }
    }

    #[test]
    fn reset_returns_to_initial() {
        let mut policy = BackoffPolicy::new();
        let _ = policy.next_delay(0.5);
        let _ = policy.next_delay(0.5);
        policy.reset();
        let d = policy.next_delay(0.5);
        assert_eq!(d, Duration::from_millis(1000));
    }

    #[test]
    fn custom_max_is_respected() {
        let mut policy = BackoffPolicy::with_max(5_000);
        for _ in 0..10 {
            let d = policy.next_delay(0.5);
            assert!(d <= Duration::from_millis(5_000));
        }
    }
}
