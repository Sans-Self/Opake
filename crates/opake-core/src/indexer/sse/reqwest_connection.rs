// Native SSE connection — wraps `reqwest::Response::bytes_stream()`.
//
// Reqwest's streaming API gives us `impl Stream<Item = Result<Bytes>>`.
// Each chunk may split mid-line, so we feed it into an
// [`SseLineAccumulator`](crate::indexer::sse::parser::SseLineAccumulator) which
// buffers partial lines and emits complete events as they parse.
//
// Unlike the WASM side (browser EventSource auto-reconnects, we just
// listen), reqwest has no built-in reconnect. All reconnect logic lives
// in the outer `SseConsumer` — this file just manages a single stream.

use std::collections::VecDeque;
use std::pin::Pin;

use futures_util::stream::{Stream, StreamExt};
use reqwest::Client;

use crate::error::Error;
use crate::indexer::sse::events::SseEvent;
use crate::indexer::sse::parser::SseLineAccumulator;
use crate::indexer::sse::transport::{SseConnection, SseTransport};

type BytesStream = Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>;

/// SSE transport for native environments. Wraps a shared [`reqwest::Client`].
#[derive(Clone)]
pub struct ReqwestSseTransport {
    client: Client,
}

impl ReqwestSseTransport {
    /// Build from an existing client. Allows sharing connection pooling
    /// with the regular XRPC transport.
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Build with a fresh default client.
    pub fn with_default_client() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for ReqwestSseTransport {
    fn default() -> Self {
        Self::with_default_client()
    }
}

impl SseTransport for ReqwestSseTransport {
    type Connection = ReqwestSseConnection;

    async fn connect(&self, indexer_url: &str, token: String) -> Result<Self::Connection, Error> {
        let url = format!(
            "{}/api/events?token={}",
            indexer_url.trim_end_matches('/'),
            urlencoding::encode(&token)
        );

        let response = self
            .client
            .get(&url)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .map_err(|e| Error::Sse(format!("SSE connect failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable>".into());
            return Err(Error::Sse(format!(
                "SSE connect returned {}: {}",
                status.as_u16(),
                body
            )));
        }

        let stream: BytesStream = Box::pin(response.bytes_stream());
        Ok(ReqwestSseConnection {
            stream,
            accumulator: SseLineAccumulator::new(),
            pending: VecDeque::new(),
        })
    }
}

/// A live native SSE connection.
pub struct ReqwestSseConnection {
    stream: BytesStream,
    accumulator: SseLineAccumulator,
    /// Events parsed from the most recent chunk but not yet returned to
    /// the caller. Drained one per `next_event` call before pulling a
    /// new chunk.
    pending: VecDeque<Result<SseEvent, Error>>,
}

impl SseConnection for ReqwestSseConnection {
    async fn next_event(&mut self) -> Result<Option<SseEvent>, Error> {
        loop {
            // Drain the pending buffer first. A per-frame parse error is NOT a
            // transport failure — log and skip it so a corrupt record can't
            // trip the consumer's reconnect loop (record-level lenience lives in
            // `SseEvent::from_name_and_data`, which now delivers poison records
            // as `CorruptRecord` events; anything still surfacing as `Err` here
            // is a malformed control frame we drop rather than reconnect on).
            while let Some(result) = self.pending.pop_front() {
                match result {
                    Ok(event) => return Ok(Some(event)),
                    Err(e) => log::warn!("[sse] skipping unparseable frame: {e}"),
                }
            }

            // Pull the next chunk from the stream.
            match self.stream.next().await {
                Some(Ok(bytes)) => {
                    let events = self.accumulator.feed_bytes(&bytes);
                    self.pending.extend(events);
                    // Fall through to the drain step on the next loop iter.
                }
                Some(Err(e)) => {
                    // The one genuine transport failure — reconnectable.
                    return Err(Error::Sse(format!("SSE stream error: {e}")));
                }
                None => {
                    // Stream ended cleanly. Per SSE spec this shouldn't
                    // happen during a healthy connection — the broadcaster
                    // keeps streams open indefinitely. Treat as reconnect
                    // signal.
                    return Ok(None);
                }
            }
        }
    }
}
