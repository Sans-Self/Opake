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

mod cleanup;
mod receive;
mod request;
mod respond;

pub use cleanup::{
    cleanup_expired_pair_requests, cleanup_pair_records, CleanupResult,
    DEFAULT_PAIR_REQUEST_TTL_SECONDS,
};
pub use receive::receive_pair_response;
pub use request::create_pair_request;
pub use respond::respond_to_pair_request;
