use log::trace;

use crate::atproto;
use crate::client::{
    get_blob_public, get_record_public, pds_from_did_document, resolve_did_document, Transport,
};
use crate::crypto::{self, ContentKey, PrivateKeyBundle};
use crate::error::Error;
use crate::keyrings::KEYRING_COLLECTION;
use crate::records::{self, Document, Encryption, Keyring};

use super::download::{decrypt_with_nonce, resolve_document_name};
use super::DOCUMENT_COLLECTION;

/// Result of downloading a keyring-encrypted document as a member.
///
/// Includes the unwrapped group key, its rotation number, and keyring rkey so
/// the caller can cache them for subsequent downloads under the same keyring.
#[derive(Debug)]
pub struct KeyringDownloadResult {
    pub filename: String,
    pub plaintext: Vec<u8>,
    pub group_key: ContentKey,
    pub keyring_rkey: String,
    pub rotation: u64,
}

/// Download and decrypt a keyring-encrypted document as a member.
///
/// This is the cross-PDS path for keyring members. In the federated workspace
/// model the document and keyring may live on different PDSes — a member-
/// uploaded document lives on the contributor's PDS, while the keyring
/// always lives on the workspace owner's. Each lookup resolves the
/// authority's PDS via DID document and uses the public XRPC endpoint.
///
/// The caller provides the document URI. The function:
/// 1. Resolves the document authority's PDS and fetches the document record.
/// 2. Resolves the keyring authority's PDS (separately, since it may differ
///    from the document host) and fetches the keyring.
/// 3. Unwraps group key → unwraps content key → decrypts blob.
pub async fn download_from_keyring_member(
    transport: &impl Transport,
    member_did: &str,
    private_keys: &PrivateKeyBundle<'_>,
    document_uri: &str,
) -> Result<KeyringDownloadResult, Error> {
    let doc_at = atproto::parse_at_uri(document_uri)?;
    if doc_at.collection != DOCUMENT_COLLECTION {
        return Err(Error::InvalidRecord(format!(
            "expected a document URI ({}), got collection {}",
            DOCUMENT_COLLECTION, doc_at.collection,
        )));
    }

    // Resolve the document host's PDS — that's the authority of the doc URI,
    // which may be the workspace owner (owner-uploaded doc) or any member
    // (contributor-uploaded doc).
    let doc_authority_did = &doc_at.authority;
    trace!("resolving PDS for document host {}", doc_authority_did);
    let doc_did_doc = resolve_did_document(transport, doc_authority_did).await?;
    let doc_pds = pds_from_did_document(&doc_did_doc)?;

    // Fetch the document record
    trace!("fetching document from {}", doc_pds);
    let doc_entry = get_record_public(
        transport,
        &doc_pds,
        doc_authority_did,
        DOCUMENT_COLLECTION,
        &doc_at.rkey,
    )
    .await?;

    let doc: Document = serde_json::from_value(doc_entry.value)?;
    records::check_version(doc.opake_version)?;

    // Must be keyring-encrypted
    let kr_enc = match &doc.encryption {
        Encryption::Keyring(kr) => kr,
        Encryption::Direct(_) => {
            return Err(Error::InvalidRecord(
                "document uses direct encryption, not keyring — \
                 use `opake download` without --keyring-member"
                    .into(),
            ));
        }
    };

    // Parse the keyring URI and resolve its authority's PDS. The keyring
    // lives on the workspace owner's PDS, which may not match the document's
    // host — federated uploads land docs on contributors' PDSes while the
    // shared keyring stays with the workspace owner.
    let kr_at = atproto::parse_at_uri(&kr_enc.keyring_ref.keyring)?;
    let kr_pds = if kr_at.authority == *doc_authority_did {
        doc_pds.clone()
    } else {
        let kr_did_doc = resolve_did_document(transport, &kr_at.authority).await?;
        pds_from_did_document(&kr_did_doc)?
    };
    trace!("fetching keyring {} from {}", kr_at.rkey, kr_pds);
    let kr_entry = get_record_public(
        transport,
        &kr_pds,
        &kr_at.authority,
        KEYRING_COLLECTION,
        &kr_at.rkey,
    )
    .await?;

    let keyring: Keyring = serde_json::from_value(kr_entry.value)?;
    records::check_version(keyring.opake_version)?;

    // Find the member's wrapped group key — check the current rotation first,
    // then fall back to key_history if the document was encrypted under an
    // older rotation.
    let doc_rotation = kr_enc.keyring_ref.rotation;
    let member_wrapped = if doc_rotation == keyring.rotation {
        keyring.members.iter().find(|m| m.did() == member_did)
    } else {
        keyring
            .key_history
            .iter()
            .find(|h| h.rotation == doc_rotation)
            .and_then(|h| h.members.iter().find(|m| m.did() == member_did))
    }
    .ok_or_else(|| {
        Error::InvalidRecord(format!(
            "DID ({member_did}) is not a member of keyring {} at rotation {doc_rotation}",
            kr_enc.keyring_ref.keyring,
        ))
    })?;

    // Asymmetric unwrap: member's private key → group key
    trace!("unwrapping group key for {}", member_did);
    let group_key = crypto::unwrap_key(
        &member_wrapped.wrapped_key,
        private_keys,
        &crypto::WrapContext::Keyring {
            uri: &kr_enc.keyring_ref.keyring,
        },
    )?;

    // Symmetric unwrap: group key → content key
    let wrapped_ck_bytes = kr_enc
        .keyring_ref
        .wrapped_content_key
        .decode()
        .map_err(|e| Error::InvalidRecord(format!("invalid wrapped content key: {e}")))?;
    let content_key = crypto::unwrap_content_key_from_keyring(&wrapped_ck_bytes, &group_key)?;

    // Fetch and decrypt the blob — co-hosted with the document record on
    // its authority's PDS (atproto convention; CIDs are repo-scoped).
    trace!(
        "fetching blob did={} cid={}",
        doc_authority_did,
        doc.blob.reference.cid
    );
    let ciphertext = get_blob_public(
        transport,
        &doc_pds,
        doc_authority_did,
        &doc.blob.reference.cid,
    )
    .await?;

    let plaintext = decrypt_with_nonce(&content_key, &kr_enc.nonce, ciphertext)?;
    let filename = resolve_document_name(&doc, &content_key)?;

    Ok(KeyringDownloadResult {
        filename,
        plaintext,
        group_key,
        keyring_rkey: kr_at.rkey,
        rotation: keyring.rotation,
    })
}

#[cfg(test)]
#[path = "download_keyring_tests.rs"]
mod tests;
