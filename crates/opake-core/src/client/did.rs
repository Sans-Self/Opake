// DID resolution and unauthenticated cross-PDS queries.
//
// These free functions take a bare `&impl Transport` — no XrpcClient, no auth.
// Used for resolving handles, fetching DID documents, and reading public
// records from other users' PDSes.

use log::trace;
use serde::Deserialize;

use super::transport::*;
use super::xrpc::{check_response, RecordEntry, RecordPage};
use crate::error::Error;

// ---------------------------------------------------------------------------
// DID document types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DidDocument {
    pub id: String,
    #[serde(default, rename = "alsoKnownAs")]
    pub also_known_as: Vec<String>,
    #[serde(default)]
    pub service: Vec<DidService>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DidService {
    pub id: String,
    #[serde(rename = "serviceEndpoint")]
    pub service_endpoint: String,
}

// ---------------------------------------------------------------------------
// Unauthenticated free functions
// ---------------------------------------------------------------------------

/// Resolve a handle to a DID via `GET https://{handle}/.well-known/atproto-did`.
/// Unauthenticated — direct HTTP to the handle's domain. Response is plain text.
pub async fn resolve_handle_wellknown(
    transport: &impl Transport,
    handle: &str,
) -> Result<String, Error> {
    trace!("resolving handle {} via .well-known/atproto-did", handle);

    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url: format!("https://{handle}/.well-known/atproto-did"),
            headers: vec![],
            body: None,
        })
        .await?;

    check_response(&response)?;

    let did = String::from_utf8(response.body)
        .map_err(|e| Error::InvalidRecord(format!("invalid UTF-8 in .well-known response: {e}")))?;
    let did = did.trim().to_string();

    if !did.starts_with("did:") {
        return Err(Error::InvalidRecord(format!(
            ".well-known/atproto-did response is not a DID: {did}"
        )));
    }

    Ok(did)
}

/// Resolve a handle to a DID via `com.atproto.identity.resolveHandle`.
/// Unauthenticated — can be called against any PDS.
pub async fn resolve_handle(
    transport: &impl Transport,
    pds_url: &str,
    handle: &str,
) -> Result<String, Error> {
    trace!("resolving handle {} via {}", handle, pds_url);

    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url: format!(
                "{}/xrpc/com.atproto.identity.resolveHandle?handle={}",
                pds_url, handle,
            ),
            headers: vec![],
            body: None,
        })
        .await?;

    check_response(&response)?;

    #[derive(Deserialize)]
    struct ResolveResponse {
        did: String,
    }

    let parsed: ResolveResponse = serde_json::from_slice(&response.body)?;
    Ok(parsed.did)
}

/// Fetch a single record from any PDS. Unauthenticated.
pub async fn get_record_public(
    transport: &impl Transport,
    pds_url: &str,
    did: &str,
    collection: &str,
    rkey: &str,
) -> Result<RecordEntry, Error> {
    trace!(
        "fetching public record {}/{}/{} from {}",
        did,
        collection,
        rkey,
        pds_url,
    );

    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url: format!(
                "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey={}",
                pds_url, did, collection, rkey,
            ),
            headers: vec![],
            body: None,
        })
        .await?;

    check_response(&response)?;
    Ok(serde_json::from_slice(&response.body)?)
}

/// Paginate through a collection on any PDS. Unauthenticated.
///
/// Like `list_collection` but uses a bare transport instead of an XrpcClient,
/// and version-checks via `opakeVersion` peek (same as `list_collection_raw`).
pub async fn list_collection_public(
    transport: &impl Transport,
    pds_url: &str,
    did: &str,
    collection: &str,
) -> Result<Vec<RecordEntry>, Error> {
    use crate::records;

    trace!(
        "listing public records {}/{} from {}",
        did,
        collection,
        pds_url,
    );

    let mut entries = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut url = format!(
            "{}/xrpc/com.atproto.repo.listRecords?repo={}&collection={}&limit=100",
            pds_url, did, collection,
        );
        if let Some(ref c) = cursor {
            url.push_str(&format!("&cursor={c}"));
        }

        let response = transport
            .send(HttpRequest {
                method: HttpMethod::Get,
                url,
                headers: vec![],
                body: None,
            })
            .await?;

        check_response(&response)?;
        let page: RecordPage = serde_json::from_slice(&response.body)?;

        for record in page.records {
            let version = match record.value.get("opakeVersion").and_then(|v| v.as_u64()) {
                Some(v) => v as u32,
                None => {
                    trace!("skipping record {} without opakeVersion", record.uri);
                    continue;
                }
            };

            if records::check_version(version).is_err() {
                trace!(
                    "skipping record {} with unsupported version {}",
                    record.uri,
                    version
                );
                continue;
            }

            entries.push(record);
        }

        match page.cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }

    Ok(entries)
}

/// Fetch a blob from any PDS by DID + CID. Unauthenticated.
pub async fn get_blob_public(
    transport: &impl Transport,
    pds_url: &str,
    did: &str,
    cid: &str,
) -> Result<Vec<u8>, Error> {
    trace!(
        "fetching public blob did={} cid={} from {}",
        did,
        cid,
        pds_url
    );

    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url: format!(
                "{}/xrpc/com.atproto.sync.getBlob?did={}&cid={}",
                pds_url, did, cid,
            ),
            headers: vec![],
            body: None,
        })
        .await?;

    check_response(&response)?;
    Ok(response.body)
}

const PLC_DIRECTORY: &str = "https://plc.directory";

/// Runtime override for the PLC directory base URL.
///
/// Environment variables are inert in browser WASM, so web clients need a
/// programmatic path to point DID resolution at a local PLC (hermetic
/// dev/test environments). Set once at startup; later calls are ignored.
static PLC_DIRECTORY_OVERRIDE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Point `did:plc` resolution at a different PLC directory.
///
/// First call wins; subsequent calls are no-ops (the resolver base is
/// process-level configuration, not per-request state). Native callers can
/// use the `OPAKE_PLC_DIRECTORY` environment variable instead.
pub fn set_plc_directory_url(url: impl Into<String>) {
    let _ = PLC_DIRECTORY_OVERRIDE.set(url.into());
}

/// Resolution order: programmatic override, `OPAKE_PLC_DIRECTORY` env var
/// (native; always absent in browser WASM), then the public directory.
fn plc_directory_url() -> String {
    resolve_plc_base(
        PLC_DIRECTORY_OVERRIDE.get().map(String::as_str),
        std::env::var("OPAKE_PLC_DIRECTORY").ok().as_deref(),
    )
}

fn resolve_plc_base(override_url: Option<&str>, env_url: Option<&str>) -> String {
    override_url
        .or(env_url)
        .unwrap_or(PLC_DIRECTORY)
        .trim_end_matches('/')
        .to_string()
}

/// Fetch a DID document from the PLC directory (did:plc) or .well-known (did:web).
pub async fn resolve_did_document(
    transport: &impl Transport,
    did: &str,
) -> Result<DidDocument, Error> {
    let url = did_document_url(did)?;

    trace!("fetching DID document from {}", url);

    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url,
            headers: vec![],
            body: None,
        })
        .await?;

    check_response(&response)?;
    Ok(serde_json::from_slice(&response.body)?)
}

/// Build the URL to fetch a DID document (PLC directory or did:web .well-known).
pub fn did_document_url(did: &str) -> Result<String, Error> {
    if did.starts_with("did:plc:") {
        let base = plc_directory_url();
        Ok(format!("{base}/{did}"))
    } else if let Some(domain) = did.strip_prefix("did:web:") {
        Ok(format!("https://{domain}/.well-known/did.json"))
    } else {
        Err(Error::InvalidRecord(format!(
            "unsupported DID method: {did}"
        )))
    }
}

/// Extract the handle from a DID document's `alsoKnownAs` field.
///
/// Returns the first entry starting with `at://`, stripped of the prefix.
pub fn handle_from_did_document(doc: &DidDocument) -> Option<String> {
    doc.also_known_as
        .iter()
        .find(|a| a.starts_with("at://"))
        .map(|a| a[5..].to_string())
}

/// Extract the PDS service endpoint (`#atproto_pds`) from a DID document.
pub fn pds_from_did_document(doc: &DidDocument) -> Result<String, Error> {
    doc.service
        .iter()
        .find(|s| s.id == "#atproto_pds")
        .map(|s| s.service_endpoint.clone())
        .ok_or_else(|| {
            Error::NotFound(format!(
                "no #atproto_pds service in DID document for {}",
                doc.id,
            ))
        })
}

#[cfg(test)]
#[path = "did_tests.rs"]
mod tests;
