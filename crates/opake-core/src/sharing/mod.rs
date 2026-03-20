// Sharing operations: create, list, and revoke grants.
//
// A grant gives another user access to a document by wrapping the document's
// content key to the recipient's public key and storing the result as an
// app.opake.grant record on the owner's PDS.

mod create;
mod heal;
mod list;
mod pending;
mod revoke;

pub use create::{create_grant, GrantParams};
pub use heal::{heal_stale_grants, HealResult};
pub use list::{list_grants, GrantEntry};
pub use pending::{
    cancel_pending_share, list_pending_shares, retry_pending_shares, PendingShareEntry,
    RetryParams, RetryResult, DEFAULT_PENDING_SHARE_TTL_SECONDS,
};
pub use revoke::revoke_grant;

pub const GRANT_COLLECTION: &str = "app.opake.grant";
