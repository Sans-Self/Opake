use crate::client::{Transport, XrpcClient};
use crate::error::Error;

use super::list::{list_documents, DocumentEntry};

/// Resolve a user-provided reference to a document's AT URI.
///
/// If `reference` already looks like an `at://` URI, it's returned as-is.
/// Otherwise it's treated as a filename: we list all documents and find the
/// matching entry. Exactly one match is required — zero or multiple matches
/// are errors.
pub async fn resolve_uri(
    client: &mut XrpcClient<impl Transport>,
    reference: &str,
) -> Result<String, Error> {
    if reference.starts_with("at://") {
        return Ok(reference.to_string());
    }

    let entries = list_documents(client).await?;
    let matches: Vec<&DocumentEntry> = entries.iter().filter(|e| e.name == reference).collect();

    match matches.len() {
        0 => Err(Error::NotFound(format!(
            "no document named {:?} — use `opake ls` to see your documents",
            reference
        ))),
        1 => Ok(matches[0].uri.clone()),
        n => {
            let uris: Vec<String> = matches.iter().map(|e| e.uri.clone()).collect();
            Err(Error::AmbiguousName {
                name: reference.to_string(),
                count: n,
                uris,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::MockTransport;

    use super::super::tests::{dummy_document, list_records_response, mock_client};

    #[tokio::test]
    async fn passthrough_at_uri() {
        let mock = MockTransport::new();
        let mut client = mock_client(mock);

        let uri = resolve_uri(
            &mut client,
            "at://did:plc:test/app.opake.cloud.document/abc",
        )
        .await
        .unwrap();
        assert_eq!(uri, "at://did:plc:test/app.opake.cloud.document/abc");
    }

    #[tokio::test]
    async fn resolves_unique_filename() {
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(
            &[
                ("a1", dummy_document("notes.txt", 100, vec![])),
                ("a2", dummy_document("photo.jpg", 200, vec![])),
            ],
            None,
        ));

        let mut client = mock_client(mock);
        let uri = resolve_uri(&mut client, "photo.jpg").await.unwrap();
        assert!(uri.contains("a2"));
    }

    #[tokio::test]
    async fn no_match_returns_not_found() {
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(
            &[("a1", dummy_document("notes.txt", 100, vec![]))],
            None,
        ));

        let mut client = mock_client(mock);
        let err = resolve_uri(&mut client, "missing.pdf").await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no document named"), "got: {msg}");
        assert!(msg.contains("opake ls"), "should suggest ls, got: {msg}");
    }

    #[tokio::test]
    async fn ambiguous_name_returns_error() {
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(
            &[
                ("a1", dummy_document("report.pdf", 100, vec![])),
                ("a2", dummy_document("report.pdf", 200, vec![])),
            ],
            None,
        ));

        let mut client = mock_client(mock);
        let err = resolve_uri(&mut client, "report.pdf").await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("report.pdf"), "got: {msg}");
        assert!(msg.contains("2"), "should mention count, got: {msg}");
    }

    #[tokio::test]
    async fn empty_collection_returns_not_found() {
        let mock = MockTransport::new();
        mock.enqueue(list_records_response(&[], None));

        let mut client = mock_client(mock);
        let err = resolve_uri(&mut client, "anything.txt").await.unwrap_err();
        assert!(err.to_string().contains("no document named"));
    }
}
