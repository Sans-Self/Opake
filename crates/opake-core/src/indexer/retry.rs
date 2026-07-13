// Bounded retry-with-backoff at the indexer-resolution boundary.
//
// The write pipeline (PDS commit → firehose → indexer → snapshot) has no
// bounded interval between a write being accepted and being queryable. An
// operation whose input depends on the indexer having consumed a prior
// own-write — resolving a keyring chain head just written, passing a
// membership check for a workspace just created — must tolerate the gap
// instead of failing on the first "not a member" / "no indexed keyring"
// response. This module is the one place that policy lives, so the window
// and backoff schedule are set once and cited from the spec.
//
// WASM-compatibility: this module never sleeps by itself. The delay is
// applied by the caller through an injected async sleep (the same pattern
// the SSE reconnect loop uses — `tokio::time::sleep` on native, `setTimeout`
// on the web), so no runtime dependency leaks into wasm builds.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::error::Error;

/// First backoff delay. Small enough that the common case — the indexer is
/// milliseconds behind — costs the user a single short beat.
pub const INITIAL_DELAY_MS: u64 = 250;

/// Per-sleep ceiling. The delay doubles until it reaches this, then holds,
/// so a long wait is a handful of attempts rather than one giant sleep.
pub const MAX_DELAY_MS: u64 = 4_000;

/// Total wait budget across all attempts. An order of magnitude above normal
/// consume lag and well below the pathological tail measured under load;
/// revisit once the lag-observability data exists (the write-visibility
/// successor design depends on that distribution).
pub const MAX_WINDOW_MS: u64 = 15_000;

/// An injected async sleep: `tokio::time::sleep` on native, a `setTimeout`
/// promise on the web. Kept out of this crate's own dependency set so
/// opake-core stays runtime-agnostic and wasm-clean.
pub type SleepFn = Box<dyn FnMut(Duration) -> Pin<Box<dyn Future<Output = ()>>>>;

/// True when an error means "the indexer has not caught up with a prior
/// own-write yet", as opposed to a genuine failure. These are the responses
/// a dependent operation absorbs within the retry window:
///
/// * `Indexer { status: 403 }` — the membership-gated chain-head endpoint
///   answering "not a member" because the genesis keyring is not indexed yet
///   (the canonical creator-first-mutation race).
/// * `Indexer { status: 404 }` — the awaited record is not in the index yet.
/// * `NotFound` — the indexer answered but has no chain head for the
///   workspace yet (the keyring field is absent from an otherwise-valid
///   chain-head response).
///
/// Everything else — chain-integrity failures, malformed records, the
/// caller's own auth failing — is surfaced immediately; retrying it would
/// only hide a real defect behind latency.
pub fn is_visibility_gap(error: &Error) -> bool {
    matches!(
        error,
        Error::NotFound(_)
            | Error::Indexer {
                status: 403 | 404,
                ..
            }
    )
}

/// Exponential-backoff schedule bounded by a total wall-clock window.
///
/// Pure and clock-driven: [`next_delay`] is handed the elapsed time since the
/// first attempt and decides whether another attempt fits inside the window.
/// The caller owns the clock and the sleeping, so this is deterministic to
/// test.
#[derive(Debug, Clone)]
pub struct VisibilityRetry {
    next_ms: u64,
    window_ms: u64,
    max_delay_ms: u64,
}

impl VisibilityRetry {
    pub fn new() -> Self {
        Self {
            next_ms: INITIAL_DELAY_MS,
            window_ms: MAX_WINDOW_MS,
            max_delay_ms: MAX_DELAY_MS,
        }
    }

    /// Decide the next backoff delay given how long the whole operation has
    /// already spent retrying. Returns `None` when the window is exhausted —
    /// the caller then surfaces [`Error::VisibilityTimeout`]. A returned
    /// delay never pushes the total past the window.
    pub fn next_delay(&mut self, elapsed_ms: u64) -> Option<Duration> {
        if elapsed_ms >= self.window_ms {
            return None;
        }
        let remaining = self.window_ms - elapsed_ms;
        let delay = self.next_ms.min(self.max_delay_ms).min(remaining);
        // Advance the schedule for the next call, saturating at the ceiling.
        self.next_ms = self.next_ms.saturating_mul(2).min(self.max_delay_ms);
        Some(Duration::from_millis(delay))
    }
}

impl Default for VisibilityRetry {
    fn default() -> Self {
        Self::new()
    }
}

/// Run `op`, retrying visibility-gap failures on the [`VisibilityRetry`]
/// schedule until it succeeds, fails for a non-gap reason, or the window is
/// exhausted (yielding [`Error::VisibilityTimeout`] naming `operation`).
///
/// `now_micros` supplies wall-clock microseconds; `sleep` applies the delay.
/// `op` must return an owned future on each call.
pub async fn retry_visibility<T, F, Fut>(
    operation: &str,
    now_micros: impl Fn() -> u64,
    sleep: &mut SleepFn,
    mut op: F,
) -> Result<T, Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    let start = now_micros();
    let mut schedule = VisibilityRetry::new();
    loop {
        match op().await {
            Ok(value) => return Ok(value),
            Err(error) if is_visibility_gap(&error) => {
                let elapsed_ms = now_micros().saturating_sub(start) / 1_000;
                match schedule.next_delay(elapsed_ms) {
                    Some(delay) => sleep(delay).await,
                    None => {
                        return Err(Error::VisibilityTimeout {
                            operation: operation.to_owned(),
                            waited_ms: elapsed_ms,
                        })
                    }
                }
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
#[path = "retry_tests.rs"]
mod tests;
