use serde::{Deserialize, Serialize};

use super::{default_version, SCHEMA_VERSION};

pub const INVITATION_ACCEPTANCE_COLLECTION: &str = "app.opake.invitationAcceptance";

/// Records that a user has accepted an invitation. Written to the
/// acceptor's own PDS.
///
/// v1: The workspace owner's UI discovers this and manually adds
/// the member. v2: The daemon auto-processes acceptances.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationAcceptance {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    /// AT URI of the invitation record being accepted.
    pub invitation: String,
    pub created_at: String,
}

impl InvitationAcceptance {
    pub fn new(invitation: String, created_at: String) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            invitation,
            created_at,
        }
    }
}
