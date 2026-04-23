// Device-to-device identity pairing via the PDS.
//
// When a user logs in on a new device, they need their X25519 identity
// keypair transferred from an existing device. Both devices are authenticated
// to the same DID, so they can read/write records in the same PDS repo.
//
// The protocol uses an ephemeral DH key exchange: the new device publishes
// an ephemeral public key, the existing device wraps the identity to that
// key, and writes the encrypted payload as a record. All records are deleted
// after the transfer completes.
//
// Key containment: the ephemeral *private* half never crosses the WASM/JS
// boundary. `create_pair_request` writes it to Storage; `try_complete_pair`
// reads it back when a matching response arrives. JS/TS code only sees the
// request uri, rkey, and public-key fingerprint — never raw key bytes.

mod cancel;
mod cleanup;
mod receive;
mod request;
mod respond;

pub use cancel::cancel_pair_request;
pub use cleanup::{
    cleanup_expired_pair_requests, cleanup_pair_records, CleanupResult,
    DEFAULT_PAIR_REQUEST_TTL_SECONDS,
};
pub use receive::{complete_pair_response, try_complete_pair};
pub use request::{create_pair_request, PairRequestInfo};
pub use respond::respond_to_pair_request;
