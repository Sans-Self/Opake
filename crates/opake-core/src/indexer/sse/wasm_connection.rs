// WASM SSE connection — wraps browser `EventSource` via web_sys.
//
// `EventSource` is callback-based, so we bridge to the `async fn next_event`
// polling API via a `futures_channel::mpsc::unbounded` channel. Each event
// listener is a `Closure` that parses the `data` payload and sends a typed
// `SseEvent` through the channel.
//
// Listener closures must outlive the `EventSource` (otherwise the JS side
// loses its callback reference and events are dropped silently). We store
// them in fields on `WasmSseConnection` and drop them on `Drop` after
// calling `es.close()`.
//
// Back-pressure: unbounded is fine for the browser. EventSource delivery
// is throttled by the browser's event loop and the server's rate limiter;
// bounded would force us into `try_send` which silently drops in a sync
// callback (no await point, no retry).

use std::cell::RefCell;
use std::rc::Rc;

use futures_channel::mpsc::{unbounded, UnboundedReceiver};
use futures_util::StreamExt;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Event, EventSource, MessageEvent};

use crate::error::Error;
use crate::indexer::sse::events::SseEvent;
use crate::indexer::sse::transport::{SseConnection, SseTransport};

/// Every named event the broadcaster can emit. Registered as individual
/// listeners because `onmessage` only fires for untyped (`event: message`)
/// frames, not typed ones.
const NAMED_EVENTS: &[&str] = &[
    "directory:upsert",
    "directory:delete",
    "document:upsert",
    "document:delete",
    "keyring:upsert",
    "keyring:delete",
    "grant:upsert",
    "grant:delete",
    "directory_update:upsert",
    "directory_update:delete",
    "keyring_update:upsert",
    "keyring_update:delete",
    "document_update:upsert",
    "document_update:delete",
];

/// SSE transport for browser environments. Stateless — all per-connection
/// state lives on [`WasmSseConnection`].
#[derive(Default, Clone)]
pub struct WasmSseTransport;

impl WasmSseTransport {
    pub fn new() -> Self {
        Self
    }
}

impl SseTransport for WasmSseTransport {
    type Connection = WasmSseConnection;

    async fn connect(&self, indexer_url: &str, token: String) -> Result<Self::Connection, Error> {
        let url = format!(
            "{}/api/events?token={}",
            indexer_url.trim_end_matches('/'),
            urlencoding::encode(&token)
        );

        let es = EventSource::new(&url).map_err(js_err)?;

        let (tx, rx) = unbounded::<Result<SseEvent, Error>>();
        // Flag set by the error closure; guards against the onerror handler
        // firing repeatedly after a single error (EventSource sometimes
        // pumps multiple error events on disconnect).
        let errored = Rc::new(RefCell::new(false));

        let mut message_closures: Vec<Closure<dyn FnMut(MessageEvent)>> = Vec::new();
        for event_name in NAMED_EVENTS {
            let tx_clone = tx.clone();
            let name = *event_name;
            let closure = Closure::wrap(Box::new(move |evt: MessageEvent| {
                let data_str = match evt.data().as_string() {
                    Some(s) => s,
                    None => {
                        log::warn!("[sse] {} event with non-string data", name);
                        return;
                    }
                };
                let result = SseEvent::from_name_and_data(name, data_str.as_bytes());
                // Channel may be closed if the connection was dropped
                // mid-flight; silently ignore.
                let _ = tx_clone.unbounded_send(result);
            }) as Box<dyn FnMut(MessageEvent)>);

            es.add_event_listener_with_callback(event_name, closure.as_ref().unchecked_ref())
                .map_err(|e| {
                    // Clean up the EventSource before returning — otherwise the
                    // browser keeps the connection open.
                    es.close();
                    js_err(e)
                })?;

            message_closures.push(closure);
        }

        // Error handler. EventSource fires onerror both on initial connect
        // failure (before we ever see a real event) and on mid-session
        // disconnect. We send an error through the channel either way —
        // the outer SseConsumer handles the difference.
        let err_tx = tx.clone();
        let errored_clone = Rc::clone(&errored);
        let error_closure = Closure::wrap(Box::new(move |_evt: Event| {
            if *errored_clone.borrow() {
                return; // already errored, ignore repeat
            }
            *errored_clone.borrow_mut() = true;
            let _ = err_tx.unbounded_send(Err(Error::Sse("EventSource error".into())));
        }) as Box<dyn FnMut(Event)>);
        es.set_onerror(Some(error_closure.as_ref().unchecked_ref()));

        Ok(WasmSseConnection {
            es,
            rx,
            _message_closures: message_closures,
            _error_closure: error_closure,
        })
    }
}

/// A live SSE connection to the indexer.
///
/// The underlying `EventSource` is closed when this value is dropped.
pub struct WasmSseConnection {
    es: EventSource,
    rx: UnboundedReceiver<Result<SseEvent, Error>>,
    // Kept alive so listeners stay registered. Never read.
    _message_closures: Vec<Closure<dyn FnMut(MessageEvent)>>,
    _error_closure: Closure<dyn FnMut(Event)>,
}

impl SseConnection for WasmSseConnection {
    async fn next_event(&mut self) -> Result<Option<SseEvent>, Error> {
        match self.rx.next().await {
            Some(Ok(event)) => Ok(Some(event)),
            Some(Err(e)) => Err(e),
            None => Ok(None), // channel closed — treat as clean EOF
        }
    }
}

impl Drop for WasmSseConnection {
    fn drop(&mut self) {
        self.es.close();
    }
}

fn js_err(e: JsValue) -> Error {
    let message = e
        .as_string()
        .or_else(|| {
            js_sys::Reflect::get(&e, &"message".into())
                .ok()?
                .as_string()
        })
        .unwrap_or_else(|| format!("{e:?}"));
    Error::Sse(message)
}
