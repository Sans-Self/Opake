// Sharing operations: create, list, and revoke grants.
//
// A grant gives another user access to a document by wrapping the document's
// content key to the recipient's public key and storing the result as an
// app.opake.grant record on the owner's PDS.

mod create;
mod list;
mod revoke;

pub use create::{create_grant, GrantParams};
pub use list::{list_grants, GrantEntry};
pub use revoke::revoke_grant;

pub const GRANT_COLLECTION: &str = "app.opake.grant";
