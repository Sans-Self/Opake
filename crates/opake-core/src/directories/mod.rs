// Directory operations: create, list, delete, manage entries.
//
// Directories are purely organizational — no crypto, no encryption. They
// own their children via an ordered AT-URI array (children-on-parent model).
// The root directory is a lazy-created singleton at rkey "self".

mod create;
mod delete;
mod entries;
mod get_or_create_root;
mod list;
mod remove;
mod tree;

pub use create::create_directory;
pub use delete::delete_directory;
pub use entries::{add_entry, remove_entry};
pub use get_or_create_root::get_or_create_root;
pub use list::{list_directories, DirectoryEntry};
pub use remove::{remove, RemoveResult};
pub use tree::{DirectoryTree, EntryKind, ResolvedPath};

pub const DIRECTORY_COLLECTION: &str = "app.opake.cloud.directory";
pub const ROOT_DIRECTORY_RKEY: &str = "self";
pub const ROOT_DIRECTORY_NAME: &str = "/";

#[cfg(test)]
pub(crate) mod tests {
    use crate::client::{HttpResponse, Session, XrpcClient};
    use crate::records::Directory;
    use crate::test_utils::MockTransport;

    use super::*;

    pub const TEST_DID: &str = "did:plc:test";

    pub fn mock_client(mock: MockTransport) -> XrpcClient<MockTransport> {
        let session = Session {
            did: TEST_DID.into(),
            handle: "test.handle".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        };
        XrpcClient::with_session(mock, "https://pds.test".into(), session)
    }

    pub fn dummy_directory(name: &str) -> Directory {
        Directory::new(name.into(), "2026-03-01T00:00:00Z".into())
    }

    pub fn dummy_directory_with_entries(name: &str, entries: Vec<String>) -> Directory {
        Directory {
            entries,
            ..Directory::new(name.into(), "2026-03-01T00:00:00Z".into())
        }
    }

    pub fn create_record_response(uri: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&serde_json::json!({
                "uri": uri,
                "cid": "bafydirectory",
            }))
            .unwrap(),
        }
    }

    pub fn put_record_response(uri: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&serde_json::json!({
                "uri": uri,
                "cid": "bafyupdated",
            }))
            .unwrap(),
        }
    }

    pub fn get_record_response(uri: &str, directory: &Directory) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&serde_json::json!({
                "uri": uri,
                "cid": "bafydirectory",
                "value": directory,
            }))
            .unwrap(),
        }
    }

    pub fn list_records_response(
        directories: &[(&str, Directory)],
        cursor: Option<&str>,
    ) -> HttpResponse {
        let records: Vec<serde_json::Value> = directories
            .iter()
            .map(|(rkey, dir)| {
                serde_json::json!({
                    "uri": format!("at://{TEST_DID}/{DIRECTORY_COLLECTION}/{rkey}"),
                    "cid": "bafydirectory",
                    "value": dir,
                })
            })
            .collect();

        let mut body = serde_json::json!({ "records": records });
        if let Some(c) = cursor {
            body["cursor"] = serde_json::Value::String(c.into());
        }

        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    pub fn not_found_response() -> HttpResponse {
        HttpResponse {
            status: 404,
            body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
        }
    }
}
