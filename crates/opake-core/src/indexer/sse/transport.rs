// SseTransport + SseConnection traits.
//
// Matches the style of `crate::client::Transport::send` — uses return-position
// `impl Future` rather than `async_trait` to avoid forcing a `Send` bound on
// futures that won't be Send on WASM (EventSource and js_sys::Function are
// both `!Send`).
//
// The shape is intentionally minimal: `SseTransport::connect` opens a
// connection, `SseConnection::next_event` polls for the next event. The
// outer `SseConsumer` handles reconnection, token refresh, and synthetic
// Reconnect events.

use crate::error::Error;
use crate::indexer::sse::events::SseEvent;
use std::future::Future;

/// Opens SSE connections against the indexer's `/api/events` endpoint.
///
/// Each call to [`connect`](Self::connect) establishes a fresh connection
/// using a one-shot token from `request_sse_token`. Reconnection is the
/// responsibility of the outer consumer, not the transport.
pub trait SseTransport {
    type Connection: SseConnection;

    /// Open a new SSE connection. The token is passed as a query parameter
    /// (EventSource can't carry custom headers) and is single-use on the
    /// indexer side, so every call must use a fresh token.
    fn connect(
        &self,
        indexer_url: &str,
        token: String,
    ) -> impl Future<Output = Result<Self::Connection, Error>>;
}

/// A live SSE connection. Poll [`next_event`](Self::next_event) in a loop
/// to consume events until it returns `Ok(None)` (clean EOF) or `Err(_)`.
pub trait SseConnection {
    /// Await the next parsed event.
    ///
    /// Returns:
    /// - `Ok(Some(event))` on a parsed frame — including
    ///   [`SseEvent::CorruptRecord`](super::events::SseEvent::CorruptRecord)
    ///   for a poison record body, which is a delivered event, not a fault
    /// - `Ok(None)` on clean connection close (rare — the broadcaster
    ///   keeps streams open indefinitely)
    /// - `Err(_)` on genuine transport failure or a closed browser tab ONLY
    ///
    /// Per-frame parse errors (a malformed control payload, a corrupt record
    /// body) are logged and skipped by the connection, never surfaced as `Err`:
    /// the consumer treats every error and clean-close as "reconnect now," so
    /// only true transport failures may trigger a reconnect.
    fn next_event(&mut self) -> impl Future<Output = Result<Option<SseEvent>, Error>>;
}
