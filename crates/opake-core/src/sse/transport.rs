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
use crate::sse::events::SseEvent;
use std::future::Future;

/// Opens SSE connections against the appview's `/api/events` endpoint.
///
/// Each call to [`connect`](Self::connect) establishes a fresh connection
/// using a one-shot token from `request_sse_token`. Reconnection is the
/// responsibility of the outer consumer, not the transport.
pub trait SseTransport {
    type Connection: SseConnection;

    /// Open a new SSE connection. The token is passed as a query parameter
    /// (EventSource can't carry custom headers) and is single-use on the
    /// appview side, so every call must use a fresh token.
    fn connect(
        &self,
        appview_url: &str,
        token: String,
    ) -> impl Future<Output = Result<Self::Connection, Error>>;
}

/// A live SSE connection. Poll [`next_event`](Self::next_event) in a loop
/// to consume events until it returns `Ok(None)` (clean EOF) or `Err(_)`.
pub trait SseConnection {
    /// Await the next parsed event.
    ///
    /// Returns:
    /// - `Ok(Some(event))` on a parsed frame
    /// - `Ok(None)` on clean connection close (rare — the broadcaster
    ///   keeps streams open indefinitely)
    /// - `Err(_)` on transport failure, parse error, or closed browser tab
    ///
    /// The consumer treats any error or clean-close as "reconnect now."
    fn next_event(&mut self) -> impl Future<Output = Result<Option<SseEvent>, Error>>;
}
