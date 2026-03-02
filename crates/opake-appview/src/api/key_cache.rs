use std::collections::HashMap;
use std::time::{Duration, Instant};

use ed25519_dalek::VerifyingKey;

use crate::error::{Error, Result};

const TTL: Duration = Duration::from_secs(300); // 5 minutes

/// Caches Ed25519 verifying keys fetched from users' PDS public key records.
pub struct KeyCache {
    entries: HashMap<String, CacheEntry>,
}

struct CacheEntry {
    key: VerifyingKey,
    fetched_at: Instant,
}

impl KeyCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Return cached key if fresh, otherwise fetch from the user's PDS.
    pub async fn get_or_fetch(&mut self, did: &str) -> Result<VerifyingKey> {
        if let Some(entry) = self.entries.get(did) {
            if entry.fetched_at.elapsed() < TTL {
                return Ok(entry.key);
            }
        }

        let key = fetch_signing_key(did).await?;
        self.entries.insert(
            did.to_string(),
            CacheEntry {
                key,
                fetched_at: Instant::now(),
            },
        );
        Ok(key)
    }
}

/// Fetch the Ed25519 signing key from a user's `app.opake.cloud.publicKey/self` record.
///
/// Resolution: DID → DID document → PDS URL → getRecord → signingKey field.
async fn fetch_signing_key(did: &str) -> Result<VerifyingKey> {
    let client = reqwest::Client::new();

    // Step 1: Resolve DID document to find PDS URL
    let did_doc_url = if did.starts_with("did:plc:") {
        format!("https://plc.directory/{did}")
    } else if did.starts_with("did:web:") {
        let host = did.strip_prefix("did:web:").unwrap_or("");
        format!("https://{host}/.well-known/did.json")
    } else {
        return Err(Error::Auth(format!("unsupported DID method: {did}")));
    };

    let did_doc: serde_json::Value = client
        .get(&did_doc_url)
        .send()
        .await
        .map_err(|e| Error::Auth(format!("failed to fetch DID document for {did}: {e}")))?
        .json()
        .await
        .map_err(|e| Error::Auth(format!("invalid DID document for {did}: {e}")))?;

    let pds_url = did_doc["service"]
        .as_array()
        .and_then(|services| {
            services.iter().find_map(|s| {
                if s["id"].as_str() == Some("#atproto_pds") {
                    s["serviceEndpoint"].as_str().map(|u| u.to_string())
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| Error::Auth(format!("no PDS service in DID document for {did}")))?;

    // Step 2: Fetch public key record from the user's PDS
    let record_url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection=app.opake.cloud.publicKey&rkey=self",
        pds_url.trim_end_matches('/'),
        did
    );

    let record_resp: serde_json::Value = client
        .get(&record_url)
        .send()
        .await
        .map_err(|e| Error::Auth(format!("failed to fetch public key record for {did}: {e}")))?
        .json()
        .await
        .map_err(|e| Error::Auth(format!("invalid public key record for {did}: {e}")))?;

    // Step 3: Extract signing key from the record
    let signing_key_b64 = record_resp["value"]["signingKey"]["$bytes"]
        .as_str()
        .ok_or_else(|| {
            Error::Auth(format!(
                "no signingKey in public key record for {did} — user needs to re-login to publish signing key"
            ))
        })?;

    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine;

    let key_bytes = BASE64
        .decode(signing_key_b64)
        .or_else(|_| {
            // PDS may strip padding
            use base64::engine::general_purpose::STANDARD_NO_PAD;
            STANDARD_NO_PAD.decode(signing_key_b64)
        })
        .map_err(|e| Error::Auth(format!("invalid base64 in signing key for {did}: {e}")))?;

    let key_array: [u8; 32] = key_bytes.try_into().map_err(|v: Vec<u8>| {
        Error::Auth(format!(
            "signing key for {did} is {} bytes, expected 32",
            v.len()
        ))
    })?;

    VerifyingKey::from_bytes(&key_array)
        .map_err(|e| Error::Auth(format!("invalid Ed25519 key for {did}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_returns_none_for_missing_key() {
        let cache = KeyCache::new();
        assert!(!cache.entries.contains_key("did:plc:unknown"));
    }
}
