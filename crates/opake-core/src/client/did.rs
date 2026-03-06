// DID resolution and unauthenticated cross-PDS queries.
//
// These free functions take a bare `&impl Transport` — no XrpcClient, no auth.
// Used for resolving handles, fetching DID documents, and reading public
// records from other users' PDSes.

use log::debug;
use serde::Deserialize;

use super::transport::*;
use super::xrpc::{check_response, RecordEntry};
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
    debug!("resolving handle {} via .well-known/atproto-did", handle);

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
    debug!("resolving handle {} via {}", handle, pds_url);

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
    debug!(
        "fetching public record {}/{}/{} from {}",
        did, collection, rkey, pds_url,
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

/// Fetch a blob from any PDS by DID + CID. Unauthenticated.
pub async fn get_blob_public(
    transport: &impl Transport,
    pds_url: &str,
    did: &str,
    cid: &str,
) -> Result<Vec<u8>, Error> {
    debug!(
        "fetching public blob did={} cid={} from {}",
        did, cid, pds_url
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

/// Fetch a DID document from the PLC directory (did:plc) or .well-known (did:web).
pub async fn resolve_did_document(
    transport: &impl Transport,
    did: &str,
) -> Result<DidDocument, Error> {
    let url = if did.starts_with("did:plc:") {
        format!("{PLC_DIRECTORY}/{did}")
    } else if let Some(domain) = did.strip_prefix("did:web:") {
        format!("https://{domain}/.well-known/did.json")
    } else {
        return Err(Error::InvalidRecord(format!(
            "unsupported DID method: {did}"
        )));
    };

    debug!("fetching DID document from {}", url);

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
