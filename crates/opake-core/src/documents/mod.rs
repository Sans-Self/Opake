// Document operations: list, upload+encrypt, download+decrypt, delete.
//
// These are the high-level building blocks that both the CLI and the future
// web Indexer use. They talk to the PDS via XrpcClient and handle record
// parsing, schema version checks, and crypto — but never touch the filesystem,
// config, or user prompts.

mod delete;
mod download;
mod download_grant;
mod download_keyring;
mod update;
mod upload;

pub use delete::delete_document;
pub use download::fetch_content_key;
pub(crate) use download::fetch_content_key_with_group_key;
pub(crate) use download::{download, download_with_group_key};
pub use download_grant::download_from_grant;
pub(crate) use download_grant::resolve_grant_metadata;
pub use download_keyring::{download_from_keyring_member, KeyringDownloadResult};
pub use update::update_content;
pub(crate) use upload::{prepare_upload, prepare_upload_keyring};
pub use upload::{KeyringUploadParams, UploadParams};

pub const DOCUMENT_COLLECTION: &str = "app.opake.document";

#[cfg(test)]
pub(crate) mod tests {
    use crate::client::{LegacySession, Session, XrpcClient};
    use crate::test_utils::MockTransport;

    pub const TEST_DID: &str = "did:plc:test";
    pub const TEST_URI: &str = "at://did:plc:test/app.opake.document/abc123";

    pub fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
        let session = Session::Legacy(LegacySession {
            did: TEST_DID.into(),
            handle: "test.handle".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        XrpcClient::with_session(mock, "https://pds.test".into(), session)
    }
}
