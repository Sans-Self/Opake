// DNS TXT handle resolution for the AT Protocol.
//
// Queries `_atproto.{handle}` for a TXT record containing `did=did:...`.
// Returns None on any failure — callers fall back to HTTP-based resolution.

use hickory_resolver::TokioAsyncResolver;
use log::debug;

/// Resolve a handle to a DID via DNS TXT record at `_atproto.{handle}`.
/// Returns `None` on any failure (timeout, NXDOMAIN, parse error).
pub async fn resolve_handle_dns(handle: &str) -> Option<String> {
    let name = format!("_atproto.{handle}");
    debug!("DNS TXT lookup: {name}");

    let resolver = TokioAsyncResolver::tokio_from_system_conf().ok()?;
    let response = resolver.txt_lookup(&name).await.ok()?;

    for record in response.iter() {
        let txt = record.to_string();
        if let Some(did) = txt.strip_prefix("did=") {
            if did.starts_with("did:") {
                debug!("DNS TXT resolved {handle} → {did}");
                return Some(did.to_string());
            }
        }
    }

    debug!("no valid did= TXT record found for {name}");
    None
}
