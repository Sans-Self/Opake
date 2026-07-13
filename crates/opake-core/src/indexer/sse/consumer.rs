// SseConsumer — the outer reconnect loop.
//
// Wraps an [`SseTransport`] and handles:
//   - Token refresh (fetch a fresh SSE token on every connect attempt)
//   - Initial connect with backoff on failure
//   - Reconnection with exponential backoff + jitter when the stream drops
//   - Synthetic `Reconnect` event emission after a previously-successful
//     connection recovers (caller treats it as "full sync the affected
//     tree to cover the gap")
//
// Callers use it like an [`SseConnection`]: poll `next_event` in a loop.
// The consumer never returns transport errors — internally it logs,
// backs off, and retries. A returned `Err` from `next_event` means
// "the consumer has been explicitly stopped" or an unrecoverable
// programming error.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::error::Error;
use crate::indexer::sse::events::SseEvent;
use crate::indexer::sse::reconnect::BackoffPolicy;
use crate::indexer::sse::transport::{SseConnection, SseTransport};

/// A future-returning token fetcher. Called before every connect attempt.
/// Own-your-captures: the closure holds clones of whatever state it needs
/// (transport, DID, signing key, indexer URL).
pub type TokenFetcher = Box<dyn FnMut() -> Pin<Box<dyn Future<Output = Result<String, Error>>>>>;

/// A future-returning sleep function. Abstracts over tokio::time::sleep
/// (native) vs setTimeout via gloo-timers (WASM), so the consumer doesn't
/// pull a runtime dependency.
pub type SleepFn = Box<dyn FnMut(Duration) -> Pin<Box<dyn Future<Output = ()>>>>;

/// A source of uniform random f64 in [0, 1) for jitter sampling. Injected
/// so tests can use a deterministic sequence.
pub type JitterRng = Box<dyn FnMut() -> f64>;

/// Configuration and state for the outer reconnect loop.
pub struct SseConsumer<T: SseTransport> {
    transport: T,
    indexer_url: String,
    fetch_token: TokenFetcher,
    sleep: SleepFn,
    jitter: JitterRng,
    backoff: BackoffPolicy,

    /// Set once any `next_event` has succeeded from the current or a
    /// previous connection. Drives whether the next successful reconnect
    /// fires a synthetic [`SseEvent::Reconnect`].
    was_connected: bool,

    /// The active connection, if any. Replaced on every reconnect.
    connection: Option<T::Connection>,

    /// Set true when the next successful reconnect should surface a
    /// synthetic Reconnect event before delivering real events.
    pending_reconnect_event: bool,
}

impl<T: SseTransport> SseConsumer<T> {
    pub fn new(
        transport: T,
        indexer_url: impl Into<String>,
        fetch_token: TokenFetcher,
        sleep: SleepFn,
        jitter: JitterRng,
    ) -> Self {
        Self {
            transport,
            indexer_url: indexer_url.into(),
            fetch_token,
            sleep,
            jitter,
            backoff: BackoffPolicy::new(),
            was_connected: false,
            connection: None,
            pending_reconnect_event: false,
        }
    }

    /// Poll for the next event.
    ///
    /// Handles reconnection internally: on disconnect or token failure,
    /// applies backoff and retries indefinitely. Returns `Ok(event)` for
    /// real events and synthetic [`SseEvent::Reconnect`] markers.
    ///
    /// The only way `next_event` returns without producing an event is
    /// if the consumer's sleep future is cancelled — typically because
    /// the entire `SseConsumer` is being dropped. In that case the caller
    /// sees a pending future that never resolves, which is the correct
    /// shutdown signal.
    pub async fn next_event(&mut self) -> Result<SseEvent, Error> {
        loop {
            // Surface a previously-queued reconnect synthetic event before
            // delivering real events. This runs exactly once per reconnect.
            if self.pending_reconnect_event {
                self.pending_reconnect_event = false;
                return Ok(SseEvent::Reconnect);
            }

            // If we have an active connection, try to pull from it.
            if let Some(conn) = self.connection.as_mut() {
                match conn.next_event().await {
                    Ok(Some(event)) => {
                        // First successful event ever seen — flip the
                        // was_connected flag so any future reconnect
                        // emits a synthetic Reconnect.
                        self.was_connected = true;
                        // Reset backoff on every successful delivery (not
                        // just connect). Connect success isn't enough —
                        // the server might 500 immediately after OK.
                        self.backoff.reset();
                        return Ok(event);
                    }
                    Ok(None) => {
                        log::info!("[sse] stream closed cleanly, reconnecting");
                        self.connection = None;
                        // Back off before reconnecting. Without this, a
                        // connect→error→reconnect cycle hammers the token
                        // endpoint unchecked and trips the rate limiter.
                        self.sleep_with_backoff().await;
                    }
                    Err(e) => {
                        log::warn!("[sse] stream error: {e}, reconnecting");
                        self.connection = None;
                        // Same backoff requirement as the Ok(None) case.
                        self.sleep_with_backoff().await;
                    }
                }
                // Fall through to reconnect (or loop back to pending_reconnect).
                continue;
            }

            // No active connection. Fetch a fresh token and reconnect.
            let token = match (self.fetch_token)().await {
                Ok(t) => t,
                Err(e) => {
                    log::warn!("[sse] token fetch failed: {e}");
                    self.sleep_with_backoff().await;
                    continue;
                }
            };

            match self.transport.connect(&self.indexer_url, token).await {
                Ok(conn) => {
                    self.connection = Some(conn);
                    // If we were previously delivering events, queue a
                    // synthetic Reconnect to surface before the next real
                    // event. Only after the initial connect is skipped —
                    // `was_connected` is only set on first event delivery.
                    if self.was_connected {
                        self.pending_reconnect_event = true;
                    }
                    // Loop back to next_event() on the new connection. We
                    // don't reset backoff here — it'll reset on the first
                    // event actually delivered (see above).
                }
                Err(e) => {
                    log::warn!("[sse] connect failed: {e}");
                    self.sleep_with_backoff().await;
                }
            }
        }
    }

    /// Sleep for the next backoff-computed delay. Advances backoff state.
    async fn sleep_with_backoff(&mut self) {
        let rand = (self.jitter)();
        let delay = self.backoff.next_delay(rand);
        log::debug!("[sse] backoff sleeping for {:?}", delay);
        (self.sleep)(delay).await;
    }

    /// Force the consumer to close its current connection. The next
    /// `next_event` call will reconnect from scratch.
    pub fn force_reconnect(&mut self) {
        self.connection = None;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::sse::events::{SseEvent, SseKeyringDeletePayload};
    use crate::indexer::sse::mock::MockSseTransport;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn delete(uri: &str) -> SseEvent {
        SseEvent::KeyringDelete(SseKeyringDeletePayload {
            uri: uri.into(),
            workspace_id: None,
            outcome: Default::default(),
        })
    }

    /// Build a consumer that uses deterministic jitter (always 0.5 → no
    /// offset) and an instant sleep (returns immediately, no real wait).
    fn build_consumer(
        transport: MockSseTransport,
        token_responses: Vec<Result<String, Error>>,
    ) -> SseConsumer<MockSseTransport> {
        let token_responses = Rc::new(RefCell::new(token_responses));

        let token_fn: TokenFetcher = Box::new(move || {
            let responses = Rc::clone(&token_responses);
            Box::pin(async move {
                let mut q = responses.borrow_mut();
                if q.is_empty() {
                    Ok("token".into())
                } else {
                    q.remove(0)
                }
            })
        });

        let sleep_fn: SleepFn = Box::new(|_| Box::pin(async {}));
        let jitter_fn: JitterRng = Box::new(|| 0.5);

        SseConsumer::new(
            transport,
            "https://example.com",
            token_fn,
            sleep_fn,
            jitter_fn,
        )
    }

    #[tokio::test]
    async fn delivers_single_event() {
        let transport = MockSseTransport::new();
        transport.push_event(delete("at://a"));

        let mut consumer = build_consumer(transport, vec![]);
        let event = consumer.next_event().await.unwrap();
        match event {
            SseEvent::KeyringDelete(d) => {
                assert_eq!(d.uri, "at://a");
            }
            _ => panic!("expected KeyringDelete"),
        }
    }

    // spec:indexer-consistency § Ordering is guaranteed per topic only
    #[tokio::test]
    async fn delivers_events_in_order() {
        let transport = MockSseTransport::new();
        transport.push_event(delete("at://a"));
        transport.push_event(delete("at://b"));
        transport.push_event(delete("at://c"));

        let mut consumer = build_consumer(transport, vec![]);

        for uri in ["at://a", "at://b", "at://c"] {
            match consumer.next_event().await.unwrap() {
                SseEvent::KeyringDelete(d) => assert_eq!(d.uri.as_str(), uri),
                _ => panic!("expected KeyringDelete"),
            }
        }
    }

    // The synthetic Reconnect is the caller's cue to full-resync, which is the
    // mechanism that closes the gap when PubSub dropped events during an outage
    // — the seam that makes sync-then-stream lose nothing.
    // spec:indexer-consistency § Snapshot and stream jointly lose nothing
    #[tokio::test]
    async fn reconnect_after_error_emits_synthetic_event() {
        let transport = MockSseTransport::new();
        // First connection: one event, then error.
        transport.push_event(delete("at://a"));
        transport.push_error(Error::Sse("drop".into()));
        // Second connection: one event.
        transport.push_event(delete("at://b"));

        let mut consumer = build_consumer(transport, vec![]);

        // First real event — flips was_connected.
        let e1 = consumer.next_event().await.unwrap();
        assert!(matches!(e1, SseEvent::KeyringDelete(ref d) if d.uri == "at://a"));

        // Connection drops (internal loop reconnects), synthetic Reconnect
        // arrives before the next real event.
        let e2 = consumer.next_event().await.unwrap();
        assert!(matches!(e2, SseEvent::Reconnect));

        // Next real event.
        let e3 = consumer.next_event().await.unwrap();
        assert!(matches!(e3, SseEvent::KeyringDelete(ref d) if d.uri == "at://b"));
    }

    #[tokio::test]
    async fn initial_connect_failure_does_not_emit_reconnect() {
        let transport = MockSseTransport::new();
        // First connect fails.
        transport.fail_next_connect(Error::Sse("initial failure".into()));
        // Second connect succeeds with one event.
        transport.push_event(delete("at://a"));

        let mut consumer = build_consumer(transport, vec![]);

        // The consumer retries internally and eventually delivers the real
        // event. Since was_connected was never true before the failure,
        // no synthetic Reconnect is emitted.
        let event = consumer.next_event().await.unwrap();
        match event {
            SseEvent::KeyringDelete(d) => assert_eq!(d.uri, "at://a"),
            SseEvent::Reconnect => panic!("unexpected synthetic reconnect on initial failure"),
            _ => panic!("unexpected event type"),
        }
    }

    #[tokio::test]
    async fn token_fetch_failure_retries_with_backoff() {
        let transport = MockSseTransport::new();
        transport.push_event(delete("at://a"));

        let mut consumer = build_consumer(
            transport,
            vec![
                Err(Error::Sse("token fetch fail 1".into())),
                Err(Error::Sse("token fetch fail 2".into())),
                Ok("good-token".into()),
            ],
        );

        // The consumer should silently retry token fetches until success,
        // then deliver the event. No error surfaces to the caller.
        let event = consumer.next_event().await.unwrap();
        match event {
            SseEvent::KeyringDelete(d) => assert_eq!(d.uri, "at://a"),
            _ => panic!("expected KeyringDelete"),
        }
    }

    #[tokio::test]
    async fn eof_triggers_reconnect() {
        let transport = MockSseTransport::new();
        transport.push_event(delete("at://a"));
        transport.push_eof(); // clean close after one event
        transport.push_event(delete("at://b"));

        let mut consumer = build_consumer(transport, vec![]);

        let e1 = consumer.next_event().await.unwrap();
        assert!(matches!(e1, SseEvent::KeyringDelete(ref d) if d.uri == "at://a"));

        let e2 = consumer.next_event().await.unwrap();
        assert!(matches!(e2, SseEvent::Reconnect));

        let e3 = consumer.next_event().await.unwrap();
        assert!(matches!(e3, SseEvent::KeyringDelete(ref d) if d.uri == "at://b"));
    }

    #[tokio::test]
    async fn tight_stream_error_loop_applies_backoff() {
        // Regression for the "infinite 429s" bug: if every freshly-opened
        // connection immediately errors out (e.g., server-side auth
        // failure), the consumer must apply backoff between reconnects
        // rather than tight-looping token requests.
        //
        // We inject a stream of Err results and track how many sleep
        // invocations occur. With the bug, sleep count == 0 (the
        // reconnect path never slept). With the fix, each failed cycle
        // triggers a sleep.

        let transport = MockSseTransport::new();
        // 5 consecutive errors, then a good event to break the loop.
        for _ in 0..5 {
            transport.push_error(Error::Sse("tight-loop".into()));
        }
        transport.push_event(delete("at://recovered"));

        let sleep_count = Rc::new(RefCell::new(0usize));
        let sleep_count_inner = Rc::clone(&sleep_count);
        let sleep_fn: SleepFn = Box::new(move |_d| {
            *sleep_count_inner.borrow_mut() += 1;
            Box::pin(async {})
        });

        let token_fn: TokenFetcher = Box::new(|| Box::pin(async { Ok("t".into()) }));
        let jitter_fn: JitterRng = Box::new(|| 0.5);
        let mut consumer = SseConsumer::new(
            transport,
            "https://example.com",
            token_fn,
            sleep_fn,
            jitter_fn,
        );

        // Drain until the good event arrives. Each error cycle should
        // have applied at least one sleep.
        let event = consumer.next_event().await.unwrap();
        assert!(matches!(event, SseEvent::KeyringDelete(_)));

        let count = *sleep_count.borrow();
        assert!(
            count >= 5,
            "expected at least 5 backoff sleeps across 5 error cycles, got {count}"
        );
    }

    #[tokio::test]
    async fn force_reconnect_triggers_reconnection() {
        let transport = MockSseTransport::new();
        transport.push_event(delete("at://a"));
        transport.push_event(delete("at://b"));

        let mut consumer = build_consumer(transport, vec![]);

        let e1 = consumer.next_event().await.unwrap();
        assert!(matches!(e1, SseEvent::KeyringDelete(_)));

        consumer.force_reconnect();

        // After force_reconnect, the next event is a synthetic Reconnect
        // (because was_connected was set), then the next real event.
        let e2 = consumer.next_event().await.unwrap();
        assert!(matches!(e2, SseEvent::Reconnect));

        let e3 = consumer.next_event().await.unwrap();
        assert!(matches!(e3, SseEvent::KeyringDelete(ref d) if d.uri == "at://b"));
    }
}
