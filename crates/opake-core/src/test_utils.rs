// Shared test infrastructure for opake-core and downstream crates.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::client::{HttpRequest, HttpResponse, Transport};
use crate::error::Error;

/// A test double for Transport that serves canned responses in FIFO order
/// and captures every request for post-hoc assertion.
#[derive(Clone)]
pub struct MockTransport {
    responses: Arc<Mutex<VecDeque<HttpResponse>>>,
    captured_requests: Arc<Mutex<Vec<HttpRequest>>>,
}

impl MockTransport {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(VecDeque::new())),
            captured_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Queue a response. Responses are served in FIFO order — first enqueued
    /// is the first returned by `send()`.
    pub fn enqueue(&self, response: HttpResponse) {
        self.responses.lock().unwrap().push_back(response);
    }

    /// All requests that were sent through this transport, in order.
    pub fn requests(&self) -> Vec<HttpRequest> {
        self.captured_requests.lock().unwrap().clone()
    }
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for MockTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, Error> {
        self.captured_requests.lock().unwrap().push(request);

        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| Error::Xrpc {
                status: 500,
                message: "MockTransport: response queue exhausted".into(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::HttpMethod;

    fn get_request(url: &str) -> HttpRequest {
        HttpRequest {
            method: HttpMethod::Get,
            url: url.into(),
            headers: vec![],
            body: None,
        }
    }

    #[tokio::test]
    async fn serves_responses_in_fifo_order() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: b"first".to_vec(),
        });
        mock.enqueue(HttpResponse {
            status: 201,
            headers: vec![],
            body: b"second".to_vec(),
        });

        let r1 = mock.send(get_request("http://a")).await.unwrap();
        let r2 = mock.send(get_request("http://b")).await.unwrap();

        assert_eq!(r1.status, 200);
        assert_eq!(r1.body, b"first");
        assert_eq!(r2.status, 201);
        assert_eq!(r2.body, b"second");
    }

    #[tokio::test]
    async fn captures_requests_in_order() {
        let mock = MockTransport::new();
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: vec![],
        });
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: vec![],
        });

        mock.send(get_request("http://first")).await.ok();
        mock.send(get_request("http://second")).await.ok();

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].url, "http://first");
        assert_eq!(reqs[1].url, "http://second");
    }

    #[tokio::test]
    async fn errors_when_queue_exhausted() {
        let mock = MockTransport::new();
        let err = mock.send(get_request("http://x")).await.unwrap_err();
        assert!(matches!(err, Error::Xrpc { status: 500, .. }));
    }
}
