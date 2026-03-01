// Sharing operations: create and revoke grants.
//
// A grant gives another user access to a document by wrapping the document's
// content key to the recipient's public key and storing the result as an
// app.opake.cloud.grant record on the owner's PDS.

mod create;
mod revoke;

pub use create::{create_grant, GrantParams};
pub use revoke::revoke_grant;

pub const GRANT_COLLECTION: &str = "app.opake.cloud.grant";
