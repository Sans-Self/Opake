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
use crate::resolve::AnchorHistory;

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
    #[serde(default, rename = "verificationMethod")]
    pub verification_methods: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DidService {
    pub id: String,
    #[serde(rename = "serviceEndpoint")]
    pub service_endpoint: String,
}

/// Failure to interpret an explicitly published verification method is never absence.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VerificationMethodError {
    #[error("malformed verification method: {0}")]
    Malformed(String),
    #[error("unsupported verification key type")]
    UnsupportedKeyType,
}

impl DidDocument {
    /// Look up a fragment in this document, rejecting duplicate or foreign methods.
    pub fn verification_method(
        &self,
        fragment: &str,
    ) -> Result<Option<&serde_json::Value>, VerificationMethodError> {
        let relative = format!("#{fragment}");
        let absolute = format!("{}{relative}", self.id);
        let mut matches = self.verification_methods.iter().filter(|method| {
            matches!(method.get("id").and_then(|id| id.as_str()), Some(id) if id == relative || id == absolute)
        });
        let method = matches.next();
        if matches.next().is_some() {
            return Err(VerificationMethodError::Malformed(
                "duplicate fragment".into(),
            ));
        }
        Ok(method)
    }

    pub fn opake_key(&self) -> Result<Option<[u8; 32]>, VerificationMethodError> {
        let Some(method) = self.verification_method("opake")? else {
            return Ok(None);
        };
        if method.get("controller").and_then(|v| v.as_str()) != Some(self.id.as_str()) {
            return Err(VerificationMethodError::Malformed(
                "foreign or missing controller".into(),
            ));
        }
        let kind = method
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| VerificationMethodError::Malformed("missing type".into()))?;
        if !matches!(kind, "Multikey" | "Ed25519VerificationKey2020") {
            return Err(VerificationMethodError::UnsupportedKeyType);
        }
        let encoded = method
            .get("publicKeyMultibase")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                VerificationMethodError::Malformed("missing publicKeyMultibase".into())
            })?;
        decode_ed25519_multibase(encoded).map(Some)
    }
}

/// Decode the base58btc multibase and ed25519-pub multicodec used by PLC.
pub fn decode_ed25519_multibase(encoded: &str) -> Result<[u8; 32], VerificationMethodError> {
    let fail = || VerificationMethodError::Malformed("invalid multibase key".into());
    let value = encoded.strip_prefix('z').ok_or_else(fail)?;
    if value.is_empty() || value.len() > 100 {
        return Err(fail());
    }
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut bytes: Vec<u8> = Vec::new();
    for digit in value.bytes() {
        let mut carry = ALPHABET.iter().position(|b| *b == digit).ok_or_else(fail)? as u32;
        for byte in bytes.iter_mut().rev() {
            carry += u32::from(*byte) * 58;
            *byte = carry as u8;
            carry >>= 8;
        }
        while carry != 0 {
            bytes.insert(0, carry as u8);
            carry >>= 8;
        }
    }
    let zeros = value.bytes().take_while(|b| *b == b'1').count();
    bytes.splice(0..0, std::iter::repeat_n(0, zeros));
    if !bytes.starts_with(&[0xed, 0x01]) {
        return Err(VerificationMethodError::UnsupportedKeyType);
    }
    let key: [u8; 32] = bytes[2..].try_into().map_err(|_| fail())?;
    ed25519_dalek::VerifyingKey::from_bytes(&key).map_err(|_| fail())?;
    Ok(key)
}

/// Encode an Ed25519 key in the `did:key` form PLC stores in its operation
/// state. Paired with [`decode_ed25519_multibase`], this lets an identity
/// operation change only its own map entry while retaining every other method.
pub fn encode_ed25519_did_key(key: &[u8; 32]) -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut bytes = vec![0xed, 0x01];
    bytes.extend_from_slice(key);
    let leading_zeros = bytes.iter().take_while(|byte| **byte == 0).count();
    let mut encoded = Vec::new();
    let mut start = leading_zeros;
    while start < bytes.len() {
        let mut remainder = 0u32;
        for byte in bytes[start..].iter_mut() {
            let value = remainder * 256 + u32::from(*byte);
            *byte = (value / 58) as u8;
            remainder = value % 58;
        }
        encoded.push(ALPHABET[remainder as usize]);
        while start < bytes.len() && bytes[start] == 0 {
            start += 1;
        }
    }
    encoded.extend(std::iter::repeat_n(b'1', leading_zeros));
    encoded.reverse();
    format!(
        "did:key:z{}",
        String::from_utf8(encoded).expect("base58 alphabet is UTF-8")
    )
}

/// An audit response status that says the directory could not answer, as
/// opposed to answering that the account is absent or the request was bad.
/// Status `0` is what the transports report for a connection failure or a
/// timeout.
fn audit_transport_unavailable(status: u16) -> bool {
    status == 0 || status == 429 || status >= 500
}

/// Read the PLC audit history. No separate remembered-key cache exists: this
/// result travels with the same resolved identity and expires with it.
pub async fn opake_anchor_history(
    transport: &impl Transport,
    did: &str,
    current: &[u8; 32],
) -> Result<AnchorHistory, Error> {
    if did.starts_with("did:web:") {
        return Ok(AnchorHistory::NoHistory);
    }
    if !did.starts_with("did:plc:") {
        return Err(Error::InvalidRecord(
            "unsupported DID history method".into(),
        ));
    }
    // A directory that cannot answer leaves the replacement question open;
    // it does not answer it in the negative, and it does not undo a signature
    // that already verified against the current document.
    // spec: account-verification § Resolution reads the anchor's history and reports a replacement
    let response = match transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url: format!("{}/{did}/log/audit", plc_directory_url()),
            headers: vec![],
            body: None,
        })
        .await
    {
        Ok(response) => response,
        Err(_) => return Ok(AnchorHistory::Unavailable),
    };
    if audit_transport_unavailable(response.status) {
        return Ok(AnchorHistory::Unavailable);
    }
    check_response(&response)?;
    // Replacement is an ever-observed security notice. The active log can
    // omit a nullified branch within PLC's recovery window, while the audit
    // endpoint retains accepted operations from that branch. The current DID
    // document remains authoritative for the key used to verify this record.
    let operations: Vec<serde_json::Value> = serde_json::from_slice(&response.body)?;
    let mut replaced = false;
    for envelope in operations {
        let operation = envelope
            .get("operation")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| Error::InvalidRecord("malformed PLC audit operation envelope".into()))?;
        let Some(methods) = operation.get("verificationMethods") else {
            // Legacy genesis operations have no verification-method map.
            continue;
        };
        let methods = methods
            .as_object()
            .ok_or_else(|| Error::InvalidRecord("malformed PLC verificationMethods map".into()))?;
        if let Some(value) = methods.get("opake") {
            let encoded = value
                .as_str()
                .and_then(|v| v.strip_prefix("did:key:"))
                .ok_or_else(|| Error::InvalidRecord("malformed PLC verification key".into()))?;
            let key = decode_ed25519_multibase(encoded)
                .map_err(|e| Error::InvalidRecord(e.to_string()))?;
            replaced |= key != *current;
        }
    }
    Ok(if replaced {
        AnchorHistory::Replaced
    } else {
        AnchorHistory::NotReplaced
    })
}

/// Read the authoritative current PLC state used to construct a replacement
/// operation. PLC treats every top-level state field as a wholesale update, so
/// callers must carry the entire response forward rather than reconstructing
/// it from a rendered DID document or recommended credentials.
pub async fn resolve_plc_state(
    transport: &impl Transport,
    did: &str,
) -> Result<serde_json::Value, Error> {
    if !did.starts_with("did:plc:") {
        return Err(Error::InvalidRecord(
            "DID-document mutation is unsupported for this DID method".into(),
        ));
    }
    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url: format!("{}/{did}/data", plc_directory_url()),
            headers: vec![],
            body: None,
        })
        .await?;
    check_response(&response)?;
    let state: serde_json::Value = serde_json::from_slice(&response.body)?;
    if !state.is_object() {
        return Err(Error::InvalidRecord("malformed PLC state response".into()));
    }
    Ok(state)
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
    let document: DidDocument = serde_json::from_slice(&response.body)?;
    if document.id != did {
        return Err(Error::InvalidRecord(
            "resolved DID document has a different subject".into(),
        ));
    }
    Ok(document)
}

/// Build the URL to fetch a DID document (PLC directory or did:web).
///
/// `did:web` follows the method's Read operation: the method-specific
/// identifier is a colon-separated path whose segments are percent-decoded and
/// rejoined with `/`. A bare authority resolves to `/.well-known/did.json`; any
/// further segments resolve to `<path>/did.json` with no well-known prefix. A
/// port is carried as a percent-encoded colon (`example.com%3A3000`).
pub fn did_document_url(did: &str) -> Result<String, Error> {
    if did.starts_with("did:plc:") {
        let base = plc_directory_url();
        Ok(format!("{base}/{did}"))
    } else if let Some(identifier) = did.strip_prefix("did:web:") {
        did_web_url(identifier, did)
    } else {
        Err(Error::InvalidRecord(format!(
            "unsupported DID method: {did}"
        )))
    }
}

fn did_web_url(identifier: &str, did: &str) -> Result<String, Error> {
    let invalid = || Error::InvalidRecord(format!("malformed did:web identifier: {did}"));
    if identifier.is_empty() {
        return Err(invalid());
    }

    let mut segments = Vec::new();
    for segment in identifier.split(':') {
        let decoded = percent_decode(segment).ok_or_else(invalid)?;
        if decoded.is_empty() || decoded.contains(['/', '?', '#', '@']) {
            return Err(invalid());
        }
        segments.push(decoded);
    }

    let path = segments.join("/");
    if segments.len() == 1 {
        Ok(format!("https://{path}/.well-known/did.json"))
    } else {
        Ok(format!("https://{path}/did.json"))
    }
}

/// Percent-decode one `did:web` path segment. Returns `None` when an escape is
/// truncated or not two hex digits, or when the result is not valid UTF-8.
fn percent_decode(segment: &str) -> Option<String> {
    let mut out = Vec::with_capacity(segment.len());
    let mut bytes = segment.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes.next()?;
            let low = bytes.next()?;
            let high = (high as char).to_digit(16)?;
            let low = (low as char).to_digit(16)?;
            out.push((high * 16 + low) as u8);
        } else {
            out.push(byte);
        }
    }
    String::from_utf8(out).ok()
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
