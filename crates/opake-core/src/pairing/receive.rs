use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::atproto;
use crate::client::{Transport, XrpcClient};
use crate::crypto::{
    decrypt_blob, unwrap_key, EncryptedPayload, MlKemPrivateKey, PrivateKeyBundle,
    X25519PrivateKey, ML_KEM_SK_LEN,
};
use crate::error::Error;
use crate::records::{
    PairResponse, PublicKeyRecord, PAIR_RESPONSE_COLLECTION, PUBLIC_KEY_COLLECTION,
    PUBLIC_KEY_RKEY,
};
use crate::storage::{Identity, Storage};

use super::cleanup::cleanup_pair_records;

/// X25519 private-key length (the classical half of the pair-state blob).
const X25519_PRIV_LEN: usize = 32;

/// Combined size of the pair-state blob persisted by `create_pair_request`.
///
/// `[X25519 private key (32) || ML-KEM-768 private key (2400)]` = 2432 bytes.
const PAIR_STATE_LEN: usize = X25519_PRIV_LEN + ML_KEM_SK_LEN;

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
) -> Result<bool, Error>
where
    T: Transport,
    S: Storage,
{
    let request_uri = format!("at://{did}/{}/{request_rkey}", crate::records::PAIR_REQUEST_COLLECTION);

    let page = client
        .list_records(PAIR_RESPONSE_COLLECTION, Some(100), None)
        .await?;

    let Some((response, response_rkey)) = find_matching_response(&page.records, &request_uri)?
    else {
        return Ok(false);
    };

    complete_pair_response(client, storage, did, request_rkey, &response, &response_rkey).await?;
    Ok(true)
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
) -> Result<(), Error>
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
    // Split the persisted blob back into its two halves. See
    // `pairing::request::create_pair_request` for the matching writer.
    let (x25519_bytes, mlkem_bytes) = state.split_at(X25519_PRIV_LEN);
    let mut x25519_priv = [0u8; X25519_PRIV_LEN];
    x25519_priv.copy_from_slice(x25519_bytes);
    let mut mlkem_priv = [0u8; ML_KEM_SK_LEN];
    mlkem_priv.copy_from_slice(mlkem_bytes);

    let identity =
        decrypt_pair_response(client, did, response, &x25519_priv, &mlkem_priv).await?;
    storage.save_identity(did, &identity).await?;

    // Tear-down is best-effort from the caller's perspective — the Identity
    // is saved, so the user is paired. A partial cleanup failure leaves
    // orphan records that `cleanup_expired_pair_requests` will sweep later.
    let _ = storage.delete_pair_state(did, request_rkey).await;
    let _ = cleanup_pair_records(client, request_rkey, response_rkey).await;

    Ok(())
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
) -> Result<Identity, Error> {
    let bundle = PrivateKeyBundle {
        x25519: ephemeral_x25519_private_key,
        ml_kem: ephemeral_ml_kem_private_key,
    };
    let content_key = unwrap_key(&response.wrapped_key, &bundle)?;

    let ciphertext = BASE64.decode(&response.ciphertext.encoded).map_err(|e| {
        Error::Decryption(format!("invalid base64 in pair response ciphertext: {e}"))
    })?;
    let nonce_bytes = BASE64
        .decode(&response.nonce.encoded)
        .map_err(|e| Error::Decryption(format!("invalid base64 in pair response nonce: {e}")))?;
    let nonce_len = nonce_bytes.len();
    let nonce: [u8; 12] = nonce_bytes.try_into().map_err(|_| {
        Error::Decryption(format!(
            "pair response nonce must be 12 bytes, got {nonce_len}"
        ))
    })?;

    let payload = EncryptedPayload { ciphertext, nonce };
    let plaintext = decrypt_blob(&content_key, &payload)?;

    let identity: Identity = serde_json::from_slice(&plaintext).map_err(|e| {
        Error::InvalidRecord(format!(
            "pair response contained invalid identity JSON: {e}"
        ))
    })?;

    let record_entry = client
        .get_record(did, PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY)
        .await?;
    let published: PublicKeyRecord = serde_json::from_value(record_entry.value)?;
    let published_key = BASE64
        .decode(&published.x25519_public_key.encoded)
        .map_err(|e| {
            Error::InvalidRecord(format!("invalid base64 in published public key: {e}"))
        })?;

    let received_key = BASE64.decode(&identity.x25519_public_key).map_err(|e| {
        Error::InvalidRecord(format!(
            "invalid base64 in received identity public key: {e}"
        ))
    })?;

    if published_key != received_key {
        return Err(Error::InvalidRecord(
            "received identity public key does not match published publicKey/self record"
                .to_string(),
        ));
    }

    Ok(identity)
}
