use log::trace;

use crate::atproto;
use crate::client::{get_blob_public, get_record_public, pds_from_did_document, resolve_did_document, Transport};
use crate::crypto;
use crate::error::Error;
use crate::records::{self, Document, Encryption};
use crate::workspace::GroupKeys;

use super::download::{decrypt_with_nonce, resolve_document_name};
use super::DOCUMENT_COLLECTION;

/// Download and decrypt a keyring-encrypted document using already-resolved
/// group keys.
///
/// This is the cross-PDS path for workspace members. A member-uploaded
/// document lives on the contributor's PDS while the owner-uploaded one lives
/// on the owner's — either way the document is fetched from its own authority's
/// public XRPC endpoint.
///
/// Crucially, this does **not** fetch the keyring or re-check membership. The
/// group key is supplied by the caller, which resolved it once at
/// workspace-resolution time — walking the keyring chain to its head (via the
/// indexer) to find the caller's current member entry and unwrap the group key
/// for every rotation they can read. That orchestration is a client concern
/// and lives in the `Opake`/`FileManager` layer; this primitive stays pure
/// crypto + transport.
///
/// The earlier self-fetching version looked the member up in the keyring
/// record named by `keyringRef.keyring` — the *stable genesis* URI — which
/// does not list members added by a later supersede. A member added after a
/// document was uploaded could therefore not open it. Keying off the resolved
/// `GroupKeys` sidesteps that entirely: the head walk already established
/// membership.
///
/// `group_keys` must carry a key for the document's `keyringRef.rotation`
/// (current or historical); otherwise the caller wasn't a member at the
/// rotation the document was encrypted under.
/// Fetch a keyring-encrypted document's keyring reference without downloading
/// the blob: the stable workspace id (genesis keyring URI) it belongs to and
/// the rotation it was encrypted under.
///
/// Used to resolve the workspace — walk its keyring chain to the current head
/// and unwrap the group key — before a cross-PDS member download, when the
/// caller starts from only a document URI.
pub async fn fetch_document_keyring_ref(
    transport: &impl Transport,
    document_uri: &str,
) -> Result<(String, u64), Error> {
    let doc_at = atproto::parse_at_uri(document_uri)?;
    if doc_at.collection != DOCUMENT_COLLECTION {
        return Err(Error::InvalidRecord(format!(
            "expected a document URI ({}), got collection {}",
            DOCUMENT_COLLECTION, doc_at.collection,
        )));
    }
    let did_doc = resolve_did_document(transport, &doc_at.authority).await?;
    let pds = pds_from_did_document(&did_doc)?;
    let entry =
        get_record_public(transport, &pds, &doc_at.authority, DOCUMENT_COLLECTION, &doc_at.rkey)
            .await?;
    let doc: Document = serde_json::from_value(entry.value)?;
    records::check_version(doc.opake_version)?;
    match &doc.encryption {
        Encryption::Keyring(kr) => {
            Ok((kr.keyring_ref.keyring.clone(), kr.keyring_ref.rotation))
        }
        Encryption::Direct(_) => Err(Error::InvalidRecord(
            "document uses direct encryption, not keyring".into(),
        )),
    }
}

pub async fn download_keyring_document(
    transport: &impl Transport,
    group_keys: GroupKeys<'_>,
    document_uri: &str,
) -> Result<(String, Vec<u8>), Error> {
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

    // Pick the group key for the rotation the document was encrypted under.
    // Absent → the caller wasn't a member at that rotation.
    let doc_rotation = kr_enc.keyring_ref.rotation;
    let group_key = group_keys.for_rotation(doc_rotation).ok_or_else(|| {
        Error::Auth(format!(
            "no group key for rotation {doc_rotation} — not a member of this workspace at that rotation"
        ))
    })?;

    // Symmetric unwrap: group key → content key
    let wrapped_ck_bytes = kr_enc
        .keyring_ref
        .wrapped_content_key
        .decode()
        .map_err(|e| Error::InvalidRecord(format!("invalid wrapped content key: {e}")))?;
    let content_key = crypto::unwrap_content_key_from_keyring(&wrapped_ck_bytes, group_key)?;

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

    Ok((filename, plaintext))
}

#[cfg(test)]
#[path = "download_keyring_tests.rs"]
mod tests;
