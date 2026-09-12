// Identity resolution: handle/DID → PDS → public key.
//
// Combines the unauthenticated client primitives into a single high-level
// call that takes a handle or DID string and returns everything needed to
// encrypt data for that user.

use log::trace;

use crate::client::{
    get_record_public, pds_from_did_document, resolve_did_document, resolve_handle,
    resolve_handle_wellknown, Transport, XrpcClient,
};

/// Public Bluesky API — used for handle resolution when no PDS is known yet.
const BSKY_PUBLIC_API: &str = "https://public.api.bsky.app";

use crate::crypto::{MlKemPublicKey, X25519PublicKey, ML_KEM_PK_LEN};
use crate::error::Error;
use crate::records::vocabulary::{self, RecordKind};
use crate::records::{PublicKeyRecord, UnreadableReason, PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY};

/// Ed25519 signing public key: 32 raw bytes.
pub type Ed25519PublicKeyBytes = [u8; 32];

/// Everything we learn about a remote user during resolution.
///
/// Carries both halves of the hybrid KEM public key. The X25519 half is in
/// `x25519_public_key`; the ML-KEM-768 half is in `ml_kem_public_key`. Both
/// are required — every published `at.opake.publicKey` record includes them.
#[derive(Debug, Clone)]
pub struct ResolvedIdentity {
    pub did: String,
    pub handle: Option<String>,
    pub pds_url: String,
    pub x25519_public_key: X25519PublicKey,
    pub x25519_algo: String,
    pub ml_kem_public_key: MlKemPublicKey,
    pub ml_kem_algo: String,
    /// Declared version of the validated public-key record.  Consumers bind
    /// approval transcripts to this record scheme instead of a build-wide
    /// constant.
    pub opake_version: u32,
    /// Ed25519 signing key — present if the user has published one.
    pub signing_key: Option<Ed25519PublicKeyBytes>,
    pub verification: VerificationState,
}

impl ResolvedIdentity {
    /// The exact commitment a caller must present after explicitly approving
    /// an unverified recipient for this document relationship.
    pub fn unverified_key_approval(&self, document_uri: &str) -> [u8; 32] {
        crate::crypto::unverified_key_approval(
            self.opake_version,
            document_uri,
            &self.did,
            &crate::crypto::EncryptionKeyFields {
                x25519_public_key: &self.x25519_public_key,
                x25519_algo: &self.x25519_algo,
                ml_kem_public_key: &self.ml_kem_public_key,
                ml_kem_algo: &self.ml_kem_algo,
            },
        )
    }
}

/// Successful resolution states. Verification failures are `Error::VerificationFailed`
/// and cannot accidentally be used as an unverified wrap target.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum VerificationState {
    Unverified,
    Verified { key_replaced: Option<bool> },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SelfVerificationState {
    Absent,
    Verified,
    Substitution,
}

/// Read the actual DID document; only absence permits offering republication.
pub async fn check_own_verification(
    transport: &impl Transport,
    did: &str,
    own_key: &[u8; 32],
) -> Result<SelfVerificationState, Error> {
    let document = resolve_did_document(transport, did).await?;
    Ok(match document.opake_key() {
        Ok(None) => SelfVerificationState::Absent,
        Ok(Some(key)) if key == *own_key => SelfVerificationState::Verified,
        Ok(Some(_)) | Err(_) => SelfVerificationState::Substitution,
    })
}

/// Shared verification boundary for resolution and received pairing identities.
/// Callers classify version and vocabulary before invoking this function.
pub async fn verify_public_key_record(
    transport: &impl Transport,
    did: &str,
    document: &crate::client::DidDocument,
    record: &PublicKeyRecord,
) -> Result<VerificationState, Error> {
    let key = document
        .opake_key()
        .map_err(|e| Error::VerificationFailed(format!("{did}: {e}")))?;
    match key {
        None => Ok(VerificationState::Unverified),
        Some(key) => {
            record
                .verify_signature(did, &key)
                .map_err(|e| Error::VerificationFailed(format!("{did}: {e}")))?;
            let key_replaced = crate::client::opake_key_replaced(transport, did, &key).await?;
            Ok(VerificationState::Verified { key_replaced })
        }
    }
}

/// Bootstrap resolution for login: resolve a handle or DID to (did, pds_url, handle)
/// without needing a known PDS.
///
/// If input starts with `did:` → fetch DID document → extract PDS + handle.
/// If input is a handle → resolve via the public Bluesky API → fetch DID document → extract PDS.
///
/// The returned handle comes from the DID document's `alsoKnownAs`, not the raw input.
pub async fn resolve_pds_for_login(
    transport: &impl Transport,
    handle_or_did: &str,
) -> Result<(String, String, Option<String>), Error> {
    let did = if handle_or_did.starts_with("did:") {
        trace!("input is already a DID: {}", handle_or_did);
        handle_or_did.to_string()
    } else {
        match resolve_handle_wellknown(transport, handle_or_did).await {
            Ok(did) => {
                trace!("resolved via .well-known: {}", did);
                did
            }
            Err(_) => {
                trace!(".well-known failed, falling back to public API");
                resolve_handle(transport, BSKY_PUBLIC_API, handle_or_did).await?
            }
        }
    };

    trace!("fetching DID document for {}", did);
    let doc = resolve_did_document(transport, &did).await?;
    let pds_url = pds_from_did_document(&doc)?;
    trace!("resolved PDS: {}", pds_url);

    let handle = doc
        .also_known_as
        .iter()
        .find_map(|alias| alias.strip_prefix("at://"))
        .map(|h| h.to_string());

    Ok((did, pds_url, handle))
}

/// Like `resolve_pds_for_login`, but tries DNS TXT resolution first.
///
/// DNS is the fastest path — a single `_atproto.{handle}` TXT lookup that
/// skips the `.well-known` and `resolveHandle` HTTP round-trips entirely.
/// Falls through to HTTP-based resolution on any DNS failure.
#[cfg(feature = "dns")]
pub async fn resolve_pds_for_login_with_dns(
    transport: &impl Transport,
    handle_or_did: &str,
) -> Result<(String, String, Option<String>), Error> {
    let identifier = if !handle_or_did.starts_with("did:") {
        crate::client::resolve_handle_dns(handle_or_did)
            .await
            .unwrap_or_else(|| handle_or_did.to_string())
    } else {
        handle_or_did.to_string()
    };
    resolve_pds_for_login(transport, &identifier).await
}

/// Full resolution: input → DID → PDS → public key.
///
/// If `input` starts with `did:`, it's used directly. Otherwise it's treated
/// as a handle and resolved against `caller_pds_url` first.
pub async fn resolve_identity(
    transport: &impl Transport,
    caller_pds_url: &str,
    input: &str,
) -> Result<ResolvedIdentity, Error> {
    // Step 1: Resolve to DID
    let did = if input.starts_with("did:") {
        trace!("input is already a DID: {}", input);
        input.to_string()
    } else {
        match resolve_handle_wellknown(transport, input).await {
            Ok(did) => {
                trace!("resolved via .well-known: {}", did);
                did
            }
            Err(_) => {
                trace!(".well-known failed, falling back to caller PDS");
                resolve_handle(transport, caller_pds_url, input).await?
            }
        }
    };

    // Step 2: Fetch DID document
    trace!("fetching DID document for {}", did);
    let doc = resolve_did_document(transport, &did).await?;

    // Step 3: Extract PDS URL
    let pds_url = pds_from_did_document(&doc)?;
    trace!("PDS for {}: {}", did, pds_url);

    // Step 4: Extract handle from alsoKnownAs
    let handle = doc
        .also_known_as
        .iter()
        .find_map(|alias| alias.strip_prefix("at://"))
        .map(|h| h.to_string());

    // Step 5: Fetch public key record.
    // A NotFound here means the DID is valid but hasn't published an Opake
    // key yet — that's a different situation from the DID/handle not existing
    // (steps 1–2). Surface it as RecipientNotReady so callers can offer a
    // pending-share queue without silently queuing shares for typo'd handles.
    trace!("fetching public key from {}", pds_url);
    let entry = get_record_public(
        transport,
        &pds_url,
        &did,
        PUBLIC_KEY_COLLECTION,
        PUBLIC_KEY_RKEY,
    )
    .await
    .map_err(|e| match e {
        Error::NotFound(_) => {
            Error::RecipientNotReady(format!("{did} has not published an Opake public key yet"))
        }
        other => other,
    })?;

    // Classify the fetched key through the shared record-validity contract:
    // version is peeked first, then a structural parse and a vocabulary check
    // against the `PublicKeyAlgo` table — which replaces the old compile-time
    // EXPECTED_*_ALGO constants (a bogus `x25519Algo`/`mlKemAlgo` is now a
    // vocabulary violation ⇒ corrupt, caught here rather than surfacing as a
    // generic crypto error at wrap time). A corrupt or future-version key
    // refuses the share with its own reason — distinct from a recipient who has
    // no key at all (RecipientNotReady above), which keeps its pending-share
    // queue semantics (see `record-validity` § "unreadable public key blocks
    // sharing with a reason").
    let record: PublicKeyRecord = vocabulary::classify_record(RecordKind::PublicKey, &entry.value)
        .map_err(|reason| match reason {
            UnreadableReason::Corrupt => Error::InvalidRecord(format!(
                "{did}'s published public key is corrupt or unreadable; cannot share with them"
            )),
            UnreadableReason::NeedsNewerClient => Error::InvalidRecord(format!(
                "{did}'s public key was written by a newer Opake version than this client \
                 supports; update your client to share with them"
            )),
        })?;

    let verification = verify_public_key_record(transport, &did, &doc, &record).await?;

    // Step 6: Decode and validate the X25519 public key.
    let key_bytes = record
        .x25519_public_key
        .decode()
        .map_err(|e| Error::InvalidRecord(format!("invalid X25519 public key: {e}")))?;
    let x25519_public_key: [u8; 32] = key_bytes.try_into().map_err(|v: Vec<u8>| {
        Error::InvalidRecord(format!(
            "X25519 public key is {} bytes, expected 32",
            v.len()
        ))
    })?;

    // Step 7: Decode and validate the ML-KEM-768 public key. FIPS-203
    // validation (`mlkem768::validate_public_key`) happens later, at the
    // wrap call sites — here we only confirm the byte length is correct.
    let mlkem_bytes = record
        .ml_kem_public_key
        .decode()
        .map_err(|e| Error::InvalidRecord(format!("invalid ML-KEM public key: {e}")))?;
    let ml_kem_public_key: MlKemPublicKey = mlkem_bytes.try_into().map_err(|v: Vec<u8>| {
        Error::InvalidRecord(format!(
            "ML-KEM public key is {} bytes, expected {ML_KEM_PK_LEN}",
            v.len()
        ))
    })?;

    // Step 8: Decode optional signing key.
    let signing_key = match record.signing_key {
        Some(ref sk) => {
            let sk_bytes = sk
                .decode()
                .map_err(|e| Error::InvalidRecord(format!("invalid signing key: {e}")))?;
            let key: [u8; 32] = sk_bytes.try_into().map_err(|v: Vec<u8>| {
                Error::InvalidRecord(format!("signing key is {} bytes, expected 32", v.len()))
            })?;
            Some(key)
        }
        None => None,
    };

    Ok(ResolvedIdentity {
        did,
        handle,
        pds_url,
        x25519_public_key,
        x25519_algo: record.x25519_algo,
        ml_kem_public_key,
        ml_kem_algo: record.ml_kem_algo,
        opake_version: record.opake_version,
        signing_key,
        verification,
    })
}

/// Publish (upsert) the user's encryption + signing public keys to their PDS.
///
/// Writes both halves of the hybrid encryption KEM (X25519 + ML-KEM-768)
/// plus the optional Ed25519 signing key. Called on every login —
/// `putRecord` is idempotent, so this is always one request regardless of
/// whether the record already exists.
pub async fn publish_public_key(
    client: &mut XrpcClient<impl Transport>,
    x25519_public_key: &X25519PublicKey,
    ml_kem_public_key: &MlKemPublicKey,
    signing_key: &crate::crypto::Ed25519SigningKey,
    created_at: &str,
) -> Result<String, Error> {
    let mut record = PublicKeyRecord::with_signing_key(
        x25519_public_key,
        ml_kem_public_key,
        &signing_key.verifying_key().to_bytes(),
        created_at,
    );
    record.sign(client.did()?, signing_key)?;
    let result = client
        .put_record(PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY, &record)
        .await?;
    Ok(result.uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::HttpResponse;
    use crate::records::{PublicKeyRecord, SCHEMA_VERSION};
    use crate::test_utils::MockTransport;

    fn success(body: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: body.as_bytes().to_vec(),
        }
    }

    fn did_document_json(did: &str, handle: &str, pds_url: &str) -> String {
        serde_json::json!({
            "id": did,
            "alsoKnownAs": [format!("at://{handle}")],
            "service": [{
                "id": "#atproto_pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": pds_url,
            }]
        })
        .to_string()
    }

    /// 1184-byte ML-KEM-768 public key full of the same byte. Real
    /// resolvers would reject this at `validate_public_key` (it's not a
    /// valid lattice element), but for resolution-flow tests we only care
    /// that the bytes round-trip the wire format correctly.
    fn dummy_ml_kem_pubkey(byte: u8) -> Vec<u8> {
        vec![byte; 1184]
    }

    fn public_key_record_json(public_key: &X25519PublicKey) -> String {
        let record = PublicKeyRecord::new(
            public_key,
            &dummy_ml_kem_pubkey(0xAA),
            "2026-03-01T00:00:00Z",
        );
        let entry = serde_json::json!({
            "uri": "at://did:plc:target/at.opake.publicKey/self",
            "cid": "bafyrecord",
            "value": record,
        });
        entry.to_string()
    }

    fn base58btc(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
        let mut digits = vec![0u8];
        for &byte in bytes {
            let mut carry = u32::from(byte);
            for digit in digits.iter_mut().rev() {
                carry += u32::from(*digit) << 8;
                *digit = (carry % 58) as u8;
                carry /= 58;
            }
            while carry != 0 {
                digits.insert(0, (carry % 58) as u8);
                carry /= 58;
            }
        }
        let leading_zeros = bytes.iter().take_while(|&&byte| byte == 0).count();
        let mut output = String::with_capacity(leading_zeros + digits.len());
        output.extend(std::iter::repeat_n('1', leading_zeros));
        output.extend(
            digits
                .into_iter()
                .map(|digit| ALPHABET[digit as usize] as char),
        );
        output
    }

    fn did_key(key: &[u8; 32]) -> String {
        let mut multicodec = vec![0xed, 0x01];
        multicodec.extend_from_slice(key);
        format!("did:key:z{}", base58btc(&multicodec))
    }

    fn anchored_did_document_json(
        did: &str,
        handle: &str,
        pds_url: &str,
        key: &[u8; 32],
    ) -> String {
        serde_json::json!({
            "id": did,
            "alsoKnownAs": [format!("at://{handle}")],
            "service": [{
                "id": "#atproto_pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": pds_url,
            }],
            "verificationMethod": [{
                "id": format!("{did}#opake"),
                "type": "Multikey",
                "controller": did,
                "publicKeyMultibase": did_key(key).strip_prefix("did:key:").unwrap(),
            }],
        })
        .to_string()
    }

    fn wellknown_404() -> HttpResponse {
        HttpResponse {
            status: 404,
            headers: vec![],
            body: b"Not Found".to_vec(),
        }
    }

    #[tokio::test]
    async fn resolve_from_handle_via_wellknown() {
        let mock = MockTransport::new();
        let pubkey = [42u8; 32];

        // 1. .well-known/atproto-did → DID (skips resolveHandle)
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: b"did:plc:target".to_vec(),
        });
        // 2. DID document
        mock.enqueue(success(&did_document_json(
            "did:plc:target",
            "alice.test",
            "https://pds.alice.example.com",
        )));
        // 3. Public key record
        mock.enqueue(success(&public_key_record_json(&pubkey)));

        let result = resolve_identity(&mock, "https://pds.caller", "alice.test")
            .await
            .unwrap();

        assert_eq!(result.did, "did:plc:target");
        assert_eq!(result.handle.as_deref(), Some("alice.test"));
        assert_eq!(result.pds_url, "https://pds.alice.example.com");
        assert_eq!(result.x25519_public_key, pubkey);

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 3);
        assert!(reqs[0].url.contains(".well-known/atproto-did"));
    }

    #[tokio::test]
    async fn resolve_from_handle_wellknown_fallback() {
        let mock = MockTransport::new();
        let pubkey = [42u8; 32];

        // 1. .well-known → 404
        mock.enqueue(wellknown_404());
        // 2. resolveHandle → DID
        mock.enqueue(success(r#"{"did":"did:plc:target"}"#));
        // 3. DID document
        mock.enqueue(success(&did_document_json(
            "did:plc:target",
            "alice.test",
            "https://pds.alice.example.com",
        )));
        // 4. Public key record
        mock.enqueue(success(&public_key_record_json(&pubkey)));

        let result = resolve_identity(&mock, "https://pds.caller", "alice.test")
            .await
            .unwrap();

        assert_eq!(result.did, "did:plc:target");
        assert_eq!(result.handle.as_deref(), Some("alice.test"));
        assert_eq!(result.pds_url, "https://pds.alice.example.com");
        assert_eq!(result.x25519_public_key, pubkey);
        assert_eq!(result.x25519_algo, "x25519");
        assert_eq!(result.ml_kem_algo, "ml-kem-768");

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 4);
        assert!(reqs[0].url.contains(".well-known/atproto-did"));
        assert!(reqs[1].url.contains("resolveHandle"));
        assert!(reqs[2].url.contains("plc.directory"));
        assert!(reqs[3].url.contains("pds.alice.example.com"));
    }

    #[tokio::test]
    async fn resolve_from_did_skips_handle_resolution() {
        let mock = MockTransport::new();
        let pubkey = [7u8; 32];

        // Only 2 requests — no resolveHandle
        mock.enqueue(success(&did_document_json(
            "did:plc:bob",
            "bob.test",
            "https://pds.bob.example.com",
        )));
        mock.enqueue(success(&public_key_record_json(&pubkey)));

        let result = resolve_identity(&mock, "https://pds.caller", "did:plc:bob")
            .await
            .unwrap();

        assert_eq!(result.did, "did:plc:bob");
        assert_eq!(result.handle.as_deref(), Some("bob.test"));
        assert_eq!(result.x25519_public_key, pubkey);

        assert_eq!(mock.requests().len(), 2);
    }

    // spec:sharing-grants § The recipient's keys are discovered from their published public-key record
    #[tokio::test]
    async fn no_public_key_record_returns_recipient_not_ready() {
        // A valid DID with no publicKey/self record is a distinct case from a
        // missing handle — the user exists but hasn't set up Opake yet.
        let mock = MockTransport::new();
        mock.enqueue(success(&did_document_json(
            "did:plc:nopubkey",
            "ghost.test",
            "https://pds.ghost",
        )));
        mock.enqueue(HttpResponse {
            status: 404,
            headers: vec![],
            body: br#"{"error":"RecordNotFound","message":"no such record"}"#.to_vec(),
        });

        let err = resolve_identity(&mock, "https://pds.caller", "did:plc:nopubkey")
            .await
            .unwrap_err();
        assert!(matches!(err, Error::RecipientNotReady(_)));
    }

    #[tokio::test]
    async fn rejects_future_schema_version() {
        let mock = MockTransport::new();
        mock.enqueue(success(&did_document_json(
            "did:plc:future",
            "future.test",
            "https://pds.future",
        )));

        let mut record = PublicKeyRecord::new(
            &[1u8; 32],
            &dummy_ml_kem_pubkey(0xCC),
            "2026-03-01T00:00:00Z",
        );
        record.opake_version = SCHEMA_VERSION + 1;
        let entry = serde_json::json!({
            "uri": "at://did:plc:future/at.opake.publicKey/self",
            "cid": "bafy",
            "value": record,
        });
        mock.enqueue(success(&entry.to_string()));

        let err = resolve_identity(&mock, "https://pds.caller", "did:plc:future")
            .await
            .unwrap_err();
        // A future-version public key refuses the share with a newer-client
        // reason (distinct from corrupt and from not-ready).
        assert!(
            matches!(err, Error::InvalidRecord(ref msg) if msg.contains("newer")),
            "expected a newer-client refusal, got: {err:?}",
        );
    }

    #[tokio::test]
    async fn publish_public_key_puts_record_and_returns_uri() {
        let mock = MockTransport::new();
        let pubkey = [55u8; 32];

        let put_response = serde_json::json!({
            "uri": "at://did:plc:test/at.opake.publicKey/self",
            "cid": "bafypublished",
        });
        mock.enqueue(success(&put_response.to_string()));

        let session = crate::client::Session::Legacy(crate::client::LegacySession {
            did: "did:plc:test".into(),
            handle: "test.handle".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        let mut client = XrpcClient::with_session(mock.clone(), "https://pds.test".into(), session);

        let signing_key = [88u8; 32];
        let mlkem_pubkey: MlKemPublicKey = [0xDDu8; 1184];
        let uri = publish_public_key(
            &mut client,
            &pubkey,
            &mlkem_pubkey,
            &crate::crypto::Ed25519SigningKey::from_bytes(&signing_key),
            "2026-03-01T12:00:00Z",
        )
        .await
        .unwrap();

        assert_eq!(uri, "at://did:plc:test/at.opake.publicKey/self");

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].url.contains("putRecord"));
    }

    /// The published record is what every other party wraps content keys to,
    /// and what recovery and pairing verify an identity against — so its
    /// shape is a contract, not an implementation detail. Assert the written
    /// body: the `at.opake.publicKey` singleton at rkey `self`, carrying
    /// both halves of the hybrid KEM under their algorithm tags. A record
    /// missing the ML-KEM half, or written under any other rkey, still
    /// returns a plausible URI — only the body catches it.
    // spec:auth-identity § The encryption public keys are published as the publicKey self-record
    #[tokio::test]
    async fn publish_public_key_writes_the_self_singleton_with_both_kem_halves() {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

        let mock = MockTransport::new();
        mock.enqueue(success(
            &serde_json::json!({
                "uri": "at://did:plc:test/at.opake.publicKey/self",
                "cid": "bafypublished",
            })
            .to_string(),
        ));

        let session = crate::client::Session::Legacy(crate::client::LegacySession {
            did: "did:plc:test".into(),
            handle: "test.handle".into(),
            access_jwt: "test-jwt".into(),
            refresh_jwt: "test-refresh".into(),
        });
        let mut client = XrpcClient::with_session(mock.clone(), "https://pds.test".into(), session);

        let x25519_pubkey = [55u8; 32];
        let ml_kem_pubkey: MlKemPublicKey = [0xDDu8; 1184];
        let signing_key = [88u8; 32];
        publish_public_key(
            &mut client,
            &x25519_pubkey,
            &ml_kem_pubkey,
            &crate::crypto::Ed25519SigningKey::from_bytes(&signing_key),
            "2026-03-01T12:00:00Z",
        )
        .await
        .unwrap();

        let reqs = mock.requests();
        let body = match &reqs[0].body {
            Some(crate::client::RequestBody::Json(v)) => v.clone(),
            _ => panic!("expected JSON body on putRecord"),
        };

        assert_eq!(body["collection"], "at.opake.publicKey");
        assert_eq!(body["rkey"], "self", "the record is a singleton at `self`");

        let record = &body["record"];
        assert_eq!(record["x25519Algo"], "x25519");
        assert_eq!(record["mlKemAlgo"], "ml-kem-768");
        assert_eq!(record["signingAlgo"], "ed25519");
        assert_eq!(
            BASE64
                .decode(record["x25519PublicKey"]["$bytes"].as_str().unwrap())
                .unwrap(),
            x25519_pubkey.to_vec(),
        );
        assert_eq!(
            BASE64
                .decode(record["mlKemPublicKey"]["$bytes"].as_str().unwrap())
                .unwrap(),
            ml_kem_pubkey.to_vec(),
            "the post-quantum half is published, not just X25519",
        );
        assert_eq!(
            BASE64
                .decode(record["signingKey"]["$bytes"].as_str().unwrap())
                .unwrap(),
            crate::crypto::Ed25519SigningKey::from_bytes(&signing_key)
                .verifying_key()
                .to_bytes()
                .to_vec(),
        );
        assert_eq!(record["signatureAlgo"], "ed25519");
        let signed: PublicKeyRecord = serde_json::from_value(record.clone()).unwrap();
        signed
            .verify_signature(
                "did:plc:test",
                &crate::crypto::Ed25519SigningKey::from_bytes(&signing_key)
                    .verifying_key()
                    .to_bytes(),
            )
            .unwrap();
    }

    #[tokio::test]
    async fn login_resolve_via_wellknown() {
        let mock = MockTransport::new();

        // 1. .well-known → DID
        mock.enqueue(HttpResponse {
            status: 200,
            headers: vec![],
            body: b"did:plc:alice".to_vec(),
        });
        // 2. DID document
        mock.enqueue(success(&did_document_json(
            "did:plc:alice",
            "alice.bsky.social",
            "https://morel.us-east.host.bsky.network",
        )));

        let (did, pds, handle) = resolve_pds_for_login(&mock, "alice.bsky.social")
            .await
            .unwrap();

        assert_eq!(did, "did:plc:alice");
        assert_eq!(pds, "https://morel.us-east.host.bsky.network");
        assert_eq!(handle.as_deref(), Some("alice.bsky.social"));

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 2);
        assert!(reqs[0].url.contains(".well-known/atproto-did"));
    }

    #[tokio::test]
    async fn login_resolve_from_handle_wellknown_fallback() {
        let mock = MockTransport::new();

        // 1. .well-known → 404
        mock.enqueue(wellknown_404());
        // 2. resolveHandle via public API → DID
        mock.enqueue(success(r#"{"did":"did:plc:alice"}"#));
        // 3. DID document
        mock.enqueue(success(&did_document_json(
            "did:plc:alice",
            "alice.bsky.social",
            "https://morel.us-east.host.bsky.network",
        )));

        let (did, pds, handle) = resolve_pds_for_login(&mock, "alice.bsky.social")
            .await
            .unwrap();

        assert_eq!(did, "did:plc:alice");
        assert_eq!(pds, "https://morel.us-east.host.bsky.network");
        assert_eq!(handle.as_deref(), Some("alice.bsky.social"));

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 3);
        assert!(reqs[0].url.contains(".well-known/atproto-did"));
        assert!(reqs[1].url.contains("resolveHandle"));
        assert!(reqs[1].url.contains("public.api.bsky.app"));
        assert!(reqs[2].url.contains("plc.directory"));
    }

    #[tokio::test]
    async fn login_resolve_from_did() {
        let mock = MockTransport::new();

        // Only 1 request — DID document, no resolveHandle
        mock.enqueue(success(&did_document_json(
            "did:plc:bob",
            "bob.test",
            "https://pds.bob.example.com",
        )));

        let (did, pds, handle) = resolve_pds_for_login(&mock, "did:plc:bob").await.unwrap();

        assert_eq!(did, "did:plc:bob");
        assert_eq!(pds, "https://pds.bob.example.com");
        assert_eq!(handle.as_deref(), Some("bob.test"));

        assert_eq!(mock.requests().len(), 1);
    }

    #[tokio::test]
    async fn login_resolve_handle_not_found() {
        let mock = MockTransport::new();

        // 1. .well-known → 404
        mock.enqueue(wellknown_404());
        // 2. resolveHandle returns 400 (unknown handle)
        mock.enqueue(HttpResponse {
            status: 400,
            headers: vec![],
            body: br#"{"error":"InvalidRequest","message":"Unable to resolve handle"}"#.to_vec(),
        });

        let err = resolve_pds_for_login(&mock, "nonexistent.invalid")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("400"), "got: {err}");
    }

    #[tokio::test]
    async fn login_resolve_did_no_pds_in_document() {
        let mock = MockTransport::new();

        // DID doc with no atproto_pds service
        mock.enqueue(success(
            &serde_json::json!({
                "id": "did:plc:nopds",
                "service": []
            })
            .to_string(),
        ));

        let err = resolve_pds_for_login(&mock, "did:plc:nopds")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("atproto_pds"), "got: {err}");
    }

    #[tokio::test]
    async fn login_resolve_did_no_also_known_as() {
        let mock = MockTransport::new();

        // DID doc with PDS but no alsoKnownAs
        mock.enqueue(success(
            &serde_json::json!({
                "id": "did:plc:nohandle",
                "service": [{
                    "id": "#atproto_pds",
                    "serviceEndpoint": "https://pds.nohandle",
                }]
            })
            .to_string(),
        ));

        let (did, pds, handle) = resolve_pds_for_login(&mock, "did:plc:nohandle")
            .await
            .unwrap();

        assert_eq!(did, "did:plc:nohandle");
        assert_eq!(pds, "https://pds.nohandle");
        assert!(handle.is_none());
    }

    #[tokio::test]
    async fn handle_without_at_prefix_gives_none() {
        let mock = MockTransport::new();
        let pubkey = [99u8; 32];

        // DID doc with no alsoKnownAs entries
        mock.enqueue(success(
            &serde_json::json!({
                "id": "did:plc:lonely",
                "service": [{
                    "id": "#atproto_pds",
                    "serviceEndpoint": "https://pds.lonely",
                }]
            })
            .to_string(),
        ));
        mock.enqueue(success(&public_key_record_json(&pubkey)));

        let result = resolve_identity(&mock, "https://pds.caller", "did:plc:lonely")
            .await
            .unwrap();
        assert!(result.handle.is_none());
    }

    /// A bogus `ml_kem_algo` like `"ml-kem-512"` paired with 1184 bytes of
    /// junk would slip past byte-length validation and fail later inside
    /// `wrap_key`'s `validate_public_key` call. Resolving should reject it
    /// upfront with a clear "wrong algorithm" error instead.
    // spec:sharing-grants § The recipient's keys are discovered from their published public-key record
    #[tokio::test]
    async fn resolve_rejects_wrong_ml_kem_algo() {
        let mock = MockTransport::new();
        let pubkey = [42u8; 32];

        mock.enqueue(success(&did_document_json(
            "did:plc:target",
            "alice.test",
            "https://pds.alice.example.com",
        )));

        // Build a public-key record with a bogus algo string.
        let mut record =
            PublicKeyRecord::new(&pubkey, &dummy_ml_kem_pubkey(0xAA), "2026-03-01T00:00:00Z");
        record.ml_kem_algo = "ml-kem-512".to_string();
        let entry = serde_json::json!({
            "uri": "at://did:plc:target/at.opake.publicKey/self",
            "cid": "bafyrecord",
            "value": record,
        });
        mock.enqueue(success(&entry.to_string()));

        let err = resolve_identity(&mock, "https://pds.caller", "did:plc:target")
            .await
            .unwrap_err();
        // A bogus `mlKemAlgo` is now a vocabulary violation ⇒ corrupt; the
        // share is refused with the corrupt-key reason.
        assert!(
            matches!(err, Error::InvalidRecord(ref msg) if msg.contains("corrupt")),
            "expected a corrupt-key refusal, got: {err:?}",
        );
    }

    // spec:sharing-grants § The recipient's keys are discovered from their published public-key record
    #[tokio::test]
    async fn resolve_rejects_wrong_x25519_algo() {
        let mock = MockTransport::new();
        let pubkey = [42u8; 32];

        mock.enqueue(success(&did_document_json(
            "did:plc:target",
            "alice.test",
            "https://pds.alice.example.com",
        )));

        let mut record =
            PublicKeyRecord::new(&pubkey, &dummy_ml_kem_pubkey(0xAA), "2026-03-01T00:00:00Z");
        record.x25519_algo = "x448".to_string();
        let entry = serde_json::json!({
            "uri": "at://did:plc:target/at.opake.publicKey/self",
            "cid": "bafyrecord",
            "value": record,
        });
        mock.enqueue(success(&entry.to_string()));

        let err = resolve_identity(&mock, "https://pds.caller", "did:plc:target")
            .await
            .unwrap_err();
        // A bogus `x25519Algo` is a vocabulary violation ⇒ corrupt.
        assert!(
            matches!(err, Error::InvalidRecord(ref msg) if msg.contains("corrupt")),
            "expected a corrupt-key refusal, got: {err:?}",
        );
    }

    #[tokio::test]
    async fn verified_record_reports_active_history_and_rejects_stripped_signature() {
        let did = "did:plc:verified";
        let signing = crate::crypto::Ed25519SigningKey::from_bytes(&[33; 32]);
        let anchor = signing.verifying_key().to_bytes();
        let doc: crate::client::DidDocument = serde_json::from_str(&anchored_did_document_json(
            did,
            "verified.test",
            "https://pds.verified.test",
            &anchor,
        ))
        .unwrap();
        let mut record = PublicKeyRecord::new(
            &[44; 32],
            &dummy_ml_kem_pubkey(0xAA),
            "2026-09-12T00:00:00Z",
        );
        record.sign(did, &signing).unwrap();

        let mock = MockTransport::new();
        mock.enqueue(success(
            &serde_json::json!([
                {"type": "plc_operation", "verificationMethods": {"opake": did_key(&anchor)}}
            ])
            .to_string(),
        ));
        assert_eq!(
            verify_public_key_record(&mock, did, &doc, &record)
                .await
                .unwrap(),
            VerificationState::Verified {
                key_replaced: Some(false)
            },
        );

        record.signature = None;
        let stripped = verify_public_key_record(&mock, did, &doc, &record)
            .await
            .unwrap_err();
        assert!(matches!(stripped, Error::VerificationFailed(_)));
    }

    #[tokio::test]
    async fn verified_record_reports_replaced_anchor_from_active_chain() {
        let did = "did:plc:verified";
        let original = crate::crypto::Ed25519SigningKey::from_bytes(&[34; 32]);
        let current = crate::crypto::Ed25519SigningKey::from_bytes(&[35; 32]);
        let current_key = current.verifying_key().to_bytes();
        let doc: crate::client::DidDocument = serde_json::from_str(&anchored_did_document_json(
            did,
            "verified.test",
            "https://pds.verified.test",
            &current_key,
        ))
        .unwrap();
        let mut record = PublicKeyRecord::new(
            &[45; 32],
            &dummy_ml_kem_pubkey(0xAA),
            "2026-09-12T00:00:00Z",
        );
        record.sign(did, &current).unwrap();

        let mock = MockTransport::new();
        mock.enqueue(success(&serde_json::json!([
            {"type": "plc_operation", "verificationMethods": {"opake": did_key(&original.verifying_key().to_bytes())}},
            {"type": "plc_operation", "verificationMethods": {}},
            {"type": "plc_operation", "verificationMethods": {"opake": did_key(&current_key)}},
        ]).to_string()));
        assert_eq!(
            verify_public_key_record(&mock, did, &doc, &record)
                .await
                .unwrap(),
            VerificationState::Verified {
                key_replaced: Some(true)
            },
        );
    }

    #[tokio::test]
    async fn corrupt_or_future_key_refuses_before_anchor_verification() {
        let did = "did:plc:future";
        let signing = crate::crypto::Ed25519SigningKey::from_bytes(&[36; 32]);
        let anchor = signing.verifying_key().to_bytes();
        let mut record = PublicKeyRecord::new(
            &[46; 32],
            &dummy_ml_kem_pubkey(0xAA),
            "2026-09-12T00:00:00Z",
        );
        record.opake_version = SCHEMA_VERSION + 1;
        let mock = MockTransport::new();
        mock.enqueue(success(&anchored_did_document_json(
            did,
            "future.test",
            "https://pds.future.test",
            &anchor,
        )));
        mock.enqueue(success(
            &serde_json::json!({
                "uri": format!("at://{did}/at.opake.publicKey/self"),
                "cid": "bafyfuture",
                "value": record,
            })
            .to_string(),
        ));

        let error = resolve_identity(&mock, "https://pds.caller", did)
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::InvalidRecord(ref message) if message.contains("newer Opake version"))
        );
        assert_eq!(
            mock.requests().len(),
            2,
            "must not fetch history after classification refusal"
        );
    }

    #[tokio::test]
    async fn own_verification_allows_only_absence_to_offer_republication() {
        let did = "did:plc:self";
        let own = crate::crypto::Ed25519SigningKey::from_bytes(&[37; 32])
            .verifying_key()
            .to_bytes();
        let other = crate::crypto::Ed25519SigningKey::from_bytes(&[38; 32])
            .verifying_key()
            .to_bytes();
        let mock = MockTransport::new();
        mock.enqueue(success(r#"{"id":"did:plc:self"}"#));
        assert_eq!(
            check_own_verification(&mock, did, &own).await.unwrap(),
            SelfVerificationState::Absent
        );

        mock.enqueue(success(&anchored_did_document_json(
            did,
            "self.test",
            "https://pds.self.test",
            &own,
        )));
        assert_eq!(
            check_own_verification(&mock, did, &own).await.unwrap(),
            SelfVerificationState::Verified
        );

        mock.enqueue(success(&anchored_did_document_json(
            did,
            "self.test",
            "https://pds.self.test",
            &other,
        )));
        assert_eq!(
            check_own_verification(&mock, did, &own).await.unwrap(),
            SelfVerificationState::Substitution
        );

        mock.enqueue(success(
            r##"{"id":"did:plc:self","verificationMethod":[{"id":"#opake"}]}"##,
        ));
        assert_eq!(
            check_own_verification(&mock, did, &own).await.unwrap(),
            SelfVerificationState::Substitution
        );
    }
}
