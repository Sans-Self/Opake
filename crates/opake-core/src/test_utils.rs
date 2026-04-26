// Shared test infrastructure for opake-core and downstream crates.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use zeroize::Zeroizing;

use crate::client::{HttpRequest, HttpResponse, Transport};
use crate::crypto::{
    MlKemPrivateKey, MlKemPublicKey, OsRng, PrivateKeyBundle, PublicKeyBundle, X25519PrivateKey,
    X25519PublicKey,
};
use crate::error::Error;
use crate::records::{AtBytes, EncryptedMetadata};
use crate::storage::Identity;

/// Owned hybrid keypair material for tests.
///
/// Tests that previously used a single X25519 keypair via `test_keypair()`
/// can now hold one of these and borrow `public_keys()` / `private_keys()`
/// to construct the bundle views that wrap/unwrap require.
pub struct TestKeys {
    pub identity: Identity,
    pub x25519_pub: X25519PublicKey,
    pub x25519_priv: Zeroizing<X25519PrivateKey>,
    pub ml_kem_pub: MlKemPublicKey,
    pub ml_kem_priv: Zeroizing<MlKemPrivateKey>,
}

impl TestKeys {
    /// Generate a fresh hybrid identity, decoded for direct borrowing.
    pub fn generate(did: &str) -> Self {
        let identity = Identity::generate(did, &mut OsRng);
        let x25519_pub = identity
            .x25519_public_key_bytes()
            .expect("test identity must have valid x25519 public key");
        let x25519_priv = identity
            .x25519_private_key_bytes()
            .expect("test identity must have valid x25519 private key");
        let ml_kem_pub = identity
            .ml_kem_public_key_bytes()
            .expect("test identity must have valid ml-kem public key");
        let ml_kem_priv = identity
            .ml_kem_private_key_bytes()
            .expect("test identity must have valid ml-kem private key");
        Self {
            identity,
            x25519_pub,
            x25519_priv,
            ml_kem_pub,
            ml_kem_priv,
        }
    }

    /// Borrow into a `PublicKeyBundle` view.
    pub fn public_keys(&self) -> PublicKeyBundle<'_> {
        PublicKeyBundle {
            x25519: &self.x25519_pub,
            ml_kem: &self.ml_kem_pub,
        }
    }

    /// Borrow into a `PrivateKeyBundle` view.
    pub fn private_keys(&self) -> PrivateKeyBundle<'_> {
        PrivateKeyBundle {
            x25519: &self.x25519_priv,
            ml_kem: &self.ml_kem_priv,
        }
    }
}

/// A no-op encrypted metadata value for tests that don't exercise decryption.
pub fn dummy_encrypted_metadata() -> EncryptedMetadata {
    EncryptedMetadata {
        ciphertext: AtBytes {
            encoded: "AAAA".into(),
        },
        nonce: AtBytes {
            encoded: "BBBB".into(),
        },
    }
}

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
