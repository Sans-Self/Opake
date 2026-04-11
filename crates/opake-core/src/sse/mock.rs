// Mock SSE connection + transport for unit and integration tests.
//
// Feeds canned events into a consumer on demand. The `push_event` method
// lets tests interleave event delivery with other async work (e.g., drive
// a TreeKeeper to apply one event, assert state, then push the next).
//
// This lives under `#[cfg(any(test, feature = "test-utils"))]` so the
// integration tests in `crates/opake-core/tests/` can construct it without
// touching opake-core internals.

use crate::error::Error;
use crate::sse::events::SseEvent;
use crate::sse::transport::{SseConnection, SseTransport};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// Shared inbox — multiple `MockSseConnection` clones observe the same queue.
#[derive(Debug, Default)]
pub struct MockInbox {
    /// Events waiting to be consumed.
    pending: VecDeque<Result<Option<SseEvent>, Error>>,
}

impl MockInbox {
    /// Push a successful event for the next `next_event` call.
    pub fn push_event(&mut self, event: SseEvent) {
        self.pending.push_back(Ok(Some(event)));
    }

    /// Push an error to simulate a transport failure. The consumer will
    /// treat this as "disconnect, reconnect now."
    pub fn push_error(&mut self, err: Error) {
        self.pending.push_back(Err(err));
    }

    /// Push a clean EOF. The consumer treats this identically to an error
    /// for reconnect purposes.
    pub fn push_eof(&mut self) {
        self.pending.push_back(Ok(None));
    }

    /// Number of events currently queued.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// True if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// Mock SSE transport. Clone-cheap — all clones share the same inbox.
#[derive(Debug, Clone, Default)]
pub struct MockSseTransport {
    inbox: Rc<RefCell<MockInbox>>,
    /// If set, `connect` returns this error instead of a connection.
    /// Used to test initial-connection-failure paths.
    connect_error: Rc<RefCell<Option<Error>>>,
}

impl MockSseTransport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push an event into the inbox. All subsequent `next_event` calls on
    /// connections created from this transport will drain from the same
    /// inbox.
    pub fn push_event(&self, event: SseEvent) {
        self.inbox.borrow_mut().push_event(event);
    }

    pub fn push_error(&self, err: Error) {
        self.inbox.borrow_mut().push_error(err);
    }

    pub fn push_eof(&self) {
        self.inbox.borrow_mut().push_eof();
    }

    /// Make the next `connect` call fail with the given error. One-shot.
    pub fn fail_next_connect(&self, err: Error) {
        *self.connect_error.borrow_mut() = Some(err);
    }

    pub fn inbox_len(&self) -> usize {
        self.inbox.borrow().len()
    }
}

impl SseTransport for MockSseTransport {
    type Connection = MockSseConnection;

    async fn connect(&self, _appview_url: &str, _token: String) -> Result<Self::Connection, Error> {
        if let Some(err) = self.connect_error.borrow_mut().take() {
            return Err(err);
        }
        Ok(MockSseConnection {
            inbox: Rc::clone(&self.inbox),
        })
    }
}

/// A connection view into the mock inbox.
#[derive(Debug)]
pub struct MockSseConnection {
    inbox: Rc<RefCell<MockInbox>>,
}

impl SseConnection for MockSseConnection {
    async fn next_event(&mut self) -> Result<Option<SseEvent>, Error> {
        // Non-blocking: if the inbox is empty, synthesize a clean EOF.
        // Tests that want to assert "consumer is waiting" should push an
        // explicit event before awaiting the consumer loop.
        match self.inbox.borrow_mut().pending.pop_front() {
            Some(result) => result,
            None => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sse::events::SseDeletePayload;

    fn delete_event(uri: &str) -> SseEvent {
        SseEvent::KeyringDelete(SseDeletePayload {
            uri: Some(uri.into()),
            directory_uri: None,
            document_uri: None,
        })
    }

    #[tokio::test]
    async fn inbox_delivers_events_in_order() {
        let transport = MockSseTransport::new();
        transport.push_event(delete_event("at://a"));
        transport.push_event(delete_event("at://b"));

        let mut conn = transport
            .connect("https://example.com", "token".into())
            .await
            .unwrap();

        let first = conn.next_event().await.unwrap().unwrap();
        assert!(matches!(
            first,
            SseEvent::KeyringDelete(ref d) if d.best_uri() == Some("at://a")
        ));

        let second = conn.next_event().await.unwrap().unwrap();
        assert!(matches!(
            second,
            SseEvent::KeyringDelete(ref d) if d.best_uri() == Some("at://b")
        ));
    }

    #[tokio::test]
    async fn empty_inbox_returns_none() {
        let transport = MockSseTransport::new();
        let mut conn = transport
            .connect("https://example.com", "token".into())
            .await
            .unwrap();
        let result = conn.next_event().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn push_error_surfaces_from_next_event() {
        let transport = MockSseTransport::new();
        transport.push_error(Error::Sse("simulated disconnect".into()));

        let mut conn = transport
            .connect("https://example.com", "token".into())
            .await
            .unwrap();
        let err = conn.next_event().await.unwrap_err();
        assert!(matches!(err, Error::Sse(_)));
    }

    #[tokio::test]
    async fn fail_next_connect_is_one_shot() {
        let transport = MockSseTransport::new();
        transport.fail_next_connect(Error::Sse("nope".into()));

        let first = transport.connect("https://example.com", "t".into()).await;
        assert!(first.is_err());

        let second = transport.connect("https://example.com", "t".into()).await;
        assert!(second.is_ok());
    }

    #[tokio::test]
    async fn cloned_transport_shares_inbox() {
        let transport = MockSseTransport::new();
        let clone = transport.clone();
        transport.push_event(delete_event("at://shared"));

        // Connect via the clone, but the event was pushed via the original.
        let mut conn = clone
            .connect("https://example.com", "t".into())
            .await
            .unwrap();
        let evt = conn.next_event().await.unwrap().unwrap();
        assert!(matches!(
            evt,
            SseEvent::KeyringDelete(ref d) if d.best_uri() == Some("at://shared")
        ));
    }
}
