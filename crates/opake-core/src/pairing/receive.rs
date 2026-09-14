use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use zeroize::Zeroizing;

use crate::atproto;
use crate::client::{resolve_did_document, Transport, XrpcClient};
use crate::crypto::{
    decrypt_blob, unwrap_key, EncryptedPayload, MlKemPrivateKey, PrivateKeyBundle,
    X25519PrivateKey, ML_KEM_SK_LEN,
};
use crate::error::Error;
use crate::records::{
    vocabulary::{self, RecordKind},
    PairResponse, PublicKeyRecord, UnreadableReason, PAIR_RESPONSE_COLLECTION,
    PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY,
};
use crate::resolve::{verify_public_key_record, VerificationState};
use crate::storage::{Identity, Storage};

use super::cleanup::cleanup_pair_records;
use super::request::PAIR_STATE_VERSION;

/// X25519 private-key length (the classical half of the pair-state blob).
const X25519_PRIV_LEN: usize = 32;

/// Total size of the versioned pair-state blob: `[VERSION(1) || X25519(32) || ML-KEM(2400)]`.
const PAIR_STATE_LEN: usize = 1 + X25519_PRIV_LEN + ML_KEM_SK_LEN;

/// Result of one pairing poll. A completed response carries the verification
/// state established while accepting the sender's identity, including a valid
/// but replaced DID anchor or a DID method with no audit history.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairCompletionResult {
    pub completed: bool,
    pub verification: Option<crate::resolve::RecipientVerificationNotice>,
}

/// Poll once for a pair response matching `request_rkey`.
///
/// Returns `true` if a matching response was found, decrypted, and the
/// resulting Identity persisted to Storage. Returns `false` if no response
/// has arrived yet — the caller's outer loop should sleep and try again.
///
/// On success this function also deletes both the request and the matching
/// response records from the PDS, and wipes the ephemeral pair state from
/// Storage. After it returns `true`, the caller can invoke the normal
/// `Opake::for_account` path and identity-requiring operations will work.
pub async fn try_complete_pair<T, S>(
    client: &mut XrpcClient<T>,
    storage: &S,
    did: &str,
    request_rkey: &str,
) -> Result<PairCompletionResult, Error>
where
    T: Transport,
    S: Storage,
{
    let request_uri = format!(
        "at://{did}/{}/{request_rkey}",
        crate::records::PAIR_REQUEST_COLLECTION
    );

    let page = client
        .list_records(PAIR_RESPONSE_COLLECTION, Some(100), None)
        .await?;

    let Some((response, response_rkey)) = find_matching_response(&page.records, &request_uri)?
    else {
        return Ok(PairCompletionResult {
            completed: false,
            verification: None,
        });
    };

    let verification = complete_pair_response(
        client,
        storage,
        did,
        request_rkey,
        &response,
        &response_rkey,
    )
    .await?;
    Ok(PairCompletionResult {
        completed: true,
        verification: Some(verification),
    })
}

/// Consume a specific pair response: decrypt the Identity, persist it, and
/// tear down both the on-PDS records and the local ephemeral key state.
///
/// Public for cases where a caller has already inspected a response (e.g.
/// the test suite) — the normal flow is `try_complete_pair`, which calls
/// this internally.
pub async fn complete_pair_response<T, S>(
    client: &mut XrpcClient<T>,
    storage: &S,
    did: &str,
    request_rkey: &str,
    response: &PairResponse,
    response_rkey: &str,
) -> Result<crate::resolve::RecipientVerificationNotice, Error>
where
    T: Transport,
    S: Storage,
{
    let state = storage.load_pair_state(did, request_rkey).await?;
    if state.len() != PAIR_STATE_LEN {
        return Err(Error::InvalidRecord(format!(
            "pair state for {request_rkey} has wrong length: expected {PAIR_STATE_LEN} bytes, got {}",
            state.len()
        )));
    }
    if state[0] != PAIR_STATE_VERSION {
        return Err(Error::InvalidRecord(format!(
            "pair state for {request_rkey} has unknown version byte: 0x{:02x}",
            state[0]
        )));
    }
    // Split the versioned blob: skip 1-byte version, then X25519(32), then ML-KEM(2400).
    // See `pairing::request::create_pair_request` for the matching writer.
    let (x25519_bytes, mlkem_bytes) = state[1..].split_at(X25519_PRIV_LEN);
    let mut x25519_priv: Zeroizing<[u8; X25519_PRIV_LEN]> = Zeroizing::new([0u8; X25519_PRIV_LEN]);
    x25519_priv.copy_from_slice(x25519_bytes);
    let mut mlkem_priv: Zeroizing<[u8; ML_KEM_SK_LEN]> = Zeroizing::new([0u8; ML_KEM_SK_LEN]);
    mlkem_priv.copy_from_slice(mlkem_bytes);

    let (identity, verification) =
        decrypt_pair_response(client, did, response, &x25519_priv, &mlkem_priv).await?;
    storage.save_identity(did, &identity).await?;

    // Tear-down is best-effort from the caller's perspective — the Identity
    // is saved, so the user is paired. A partial cleanup failure leaves
    // orphan records that `cleanup_expired_pair_requests` will sweep later.
    let _ = storage.delete_pair_state(did, request_rkey).await;
    let _ = cleanup_pair_records(client, request_rkey, response_rkey).await;

    Ok(crate::resolve::RecipientVerificationNotice {
        did: did.to_owned(),
        verification,
    })
}

fn find_matching_response(
    entries: &[crate::client::RecordEntry],
    request_uri: &str,
) -> Result<Option<(PairResponse, String)>, Error> {
    for entry in entries {
        let Ok(response) = serde_json::from_value::<PairResponse>(entry.value.clone()) else {
            continue;
        };
        if response.request == request_uri {
            let rkey = atproto::parse_at_uri(&entry.uri)?.rkey;
            return Ok(Some((response, rkey)));
        }
    }
    Ok(None)
}

/// Decrypt a pair response to recover the sender's Identity.
///
/// Verifies the decrypted public key against the sender's published
/// `publicKey/self` record so an attacker who can intercept PDS writes
/// cannot swap in a different identity.
async fn decrypt_pair_response(
    client: &mut XrpcClient<impl Transport>,
    did: &str,
    response: &PairResponse,
    ephemeral_x25519_private_key: &X25519PrivateKey,
    ephemeral_ml_kem_private_key: &MlKemPrivateKey,
) -> Result<(Identity, VerificationState), Error> {
    let bundle = PrivateKeyBundle {
        x25519: ephemeral_x25519_private_key,
        ml_kem: ephemeral_ml_kem_private_key,
    };
    let content_key = unwrap_key(
        &response.wrapped_key,
        &bundle,
        &crate::crypto::WrapContext::PairResponse,
        response.opake_version,
    )?;

    let ciphertext = response.ciphertext.decode().map_err(|e| {
        Error::Decryption(format!("invalid base64 in pair response ciphertext: {e}"))
    })?;
    let nonce_bytes = response
        .nonce
        .decode()
        .map_err(|e| Error::Decryption(format!("invalid base64 in pair response nonce: {e}")))?;
    let nonce_len = nonce_bytes.len();
    let nonce: [u8; 12] = nonce_bytes.try_into().map_err(|_| {
        Error::Decryption(format!(
            "pair response nonce must be 12 bytes, got {nonce_len}"
        ))
    })?;

    let payload = EncryptedPayload { ciphertext, nonce };
    // spec:document-crypto § Key-carrying types zeroize on drop
    let seal_context = crate::crypto::SealContext::pair_identity();
    let plaintext = Zeroizing::new(decrypt_blob(&content_key, &payload, &seal_context)?);

    let identity: Identity = serde_json::from_slice(&plaintext).map_err(|e| {
        Error::InvalidRecord(format!(
            "pair response contained invalid identity JSON: {e}"
        ))
    })?;

    if identity.did != did {
        return Err(Error::InvalidRecord(
            "pair response identity DID does not match the pairing account".to_string(),
        ));
    }

    // Resolve the anchor independently of the PDS record. A DID document with
    // `#opake` turns a bad or stripped record into refusal, never downgrade.
    let document = resolve_did_document(client.transport(), did).await?;
    let record_entry = client
        .get_record(did, PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY)
        .await?;
    let published: PublicKeyRecord =
        vocabulary::classify_record(RecordKind::PublicKey, &record_entry.value).map_err(
            |reason| match reason {
                UnreadableReason::Corrupt => Error::InvalidRecord(
                    "published publicKey/self record is corrupt or unreadable".to_string(),
                ),
                UnreadableReason::NeedsNewerClient => Error::InvalidRecord(
                    "published publicKey/self record requires a newer Opake client".to_string(),
                ),
            },
        )?;
    let verification =
        verify_public_key_record(client.transport(), did, &document, &published).await?;

    let published_x25519 = published.x25519_public_key.decode().map_err(|e| {
        Error::InvalidRecord(format!(
            "invalid base64 in published X25519 public key: {e}"
        ))
    })?;
    let received_x25519 = BASE64.decode(&identity.x25519_public_key).map_err(|e| {
        Error::InvalidRecord(format!(
            "invalid base64 in received identity X25519 public key: {e}"
        ))
    })?;
    if published_x25519 != received_x25519 {
        return Err(Error::InvalidRecord(
            "received X25519 public key does not match published publicKey/self record".to_string(),
        ));
    }

    let published_ml_kem = published.ml_kem_public_key.decode().map_err(|e| {
        Error::InvalidRecord(format!(
            "invalid base64 in published ML-KEM public key: {e}"
        ))
    })?;
    let received_ml_kem = BASE64.decode(&identity.ml_kem_public_key).map_err(|e| {
        Error::InvalidRecord(format!(
            "invalid base64 in received identity ML-KEM public key: {e}"
        ))
    })?;
    if published_ml_kem != received_ml_kem {
        return Err(Error::InvalidRecord(
            "received ML-KEM public key does not match published publicKey/self record".to_string(),
        ));
    }

    if matches!(verification, VerificationState::Verified { .. }) {
        let published_signing = published
            .signing_key
            .as_ref()
            .ok_or_else(|| {
                Error::VerificationFailed(
                    "verified publicKey/self record omitted its signing key".to_string(),
                )
            })?
            .decode()?;
        let received_signing = identity.verify_key_bytes()?.ok_or_else(|| {
            Error::VerificationFailed(
                "paired identity omitted its verification key for a verified account".to_string(),
            )
        })?;
        if published_signing.as_slice() != received_signing {
            return Err(Error::VerificationFailed(
                "received signing key does not match verified publicKey/self record".to_string(),
            ));
        }
    }

    Ok((identity, verification))
}

#[cfg(test)]
#[path = "receive_tests.rs"]
mod tests;
