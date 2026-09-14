// Re-wrap sweep: migrate document content-key wraps from a historical group
// key to the workspace's current one after a rotation.
//
// This is background hygiene under the background-work contract, not a
// correctness step. A rotation is already complete when the keyring supersede
// lands: forward secrecy holds against the removed member, and every remaining
// member reads every document via `keyHistory`. The sweep only *bounds* the
// key-history walk a reader performs — it has no security effect, because a
// re-wrap cannot revoke anything a former member could already unwrap.
//
// Only the per-document AES content-key wrap is rewritten; the blob ciphertext
// is untouched (per-document content keys are wrapped under the group key
// precisely so rotation never re-encrypts blobs — CLAUDE.md decision 3).
//
// spec:key-rotation § The re-wrap sweep is hygiene under the background-work contract
// spec:background-work § Concurrency is resolved per record by compare-and-swap
//
// No daemon schedules this sweep. Since members may be admitted without a
// current wrap, a document sweep cannot infer from a `Workspace` key alone that
// every admitted member holds the live key, and replacing a document's sole
// wrap would strip historical-only members of access they legitimately have.
// Re-enabling it requires a fresh-head, per-item exclusion guard at the write
// boundary; until then the planner is kept for its tests and nothing else.
// spec:workspace-membership § Membership state is the keyring head's member list

use base64::Engine;
use log::trace;

use crate::atproto::{self, AtBytes};
use crate::client::{Transport, XrpcClient};
use crate::documents::DOCUMENT_COLLECTION;
use crate::error::Error;
use crate::records;
use crate::workspace::GroupKeys;

/// What re-wrapping a single document produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewrapItem {
    /// The content-key wrap was migrated to the head rotation.
    Rewrapped,
    /// Already at the head rotation — nothing to migrate.
    AlreadyCurrent,
    /// Not a keyring-encrypted document for this workspace — skipped.
    NotApplicable,
    /// A concurrent writer moved the record first; re-deriving finds no work,
    /// so this runner skips. A CAS conflict is never an error.
    Conflict,
}

/// Tally across a sweep pass over one workspace's documents.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RewrapOutcome {
    pub rewrapped: usize,
    pub already_current: usize,
    pub conflicts: usize,
    pub not_applicable: usize,
}

/// The decision for one document, computed purely from the record and the
/// current head key material — no network.
#[derive(Debug)]
pub enum RewrapPlan {
    /// Not a keyring document for this workspace.
    NotApplicable,
    /// The wrap already targets the head rotation.
    AlreadyCurrent,
    /// The record to write, with its content key re-wrapped under the head
    /// group key and its `keyringRef.rotation` advanced to the head.
    Rewrap(Box<records::Document>),
}

/// Plan a document's re-wrap against the head key material.
///
/// `head` is re-resolved by the caller at planning time, so a head that has
/// advanced since a sweep began yields a re-wrap to the *current* rotation,
/// never a write onto a superseded one. Returns [`RewrapPlan::AlreadyCurrent`]
/// when the wrap already targets the head — the common steady-state outcome
/// and what a re-derive after a CAS conflict lands on.
///
/// spec:background-work § Concurrency is resolved per record by compare-and-swap
pub fn plan_rewrap(
    document: &records::Document,
    workspace_id: &str,
    head: GroupKeys<'_>,
) -> Result<RewrapPlan, Error> {
    let ke = match &document.encryption {
        records::Encryption::Keyring(ke) if ke.keyring_ref.keyring == workspace_id => ke,
        _ => return Ok(RewrapPlan::NotApplicable),
    };

    let doc_rotation = ke.keyring_ref.rotation;
    if doc_rotation == head.current_rotation {
        return Ok(RewrapPlan::AlreadyCurrent);
    }

    let old_key = head.for_rotation(doc_rotation).ok_or_else(|| {
        Error::Decryption(format!(
            "rewrap: no group key for rotation {doc_rotation} of {workspace_id}"
        ))
    })?;
    let wrapped_bytes = ke
        .keyring_ref
        .wrapped_content_key
        .decode()
        .map_err(|e| Error::Decryption(format!("rewrap: invalid wrapped content key: {e}")))?;
    let content_key = crate::crypto::unwrap_content_key_from_keyring(&wrapped_bytes, old_key)?;
    let current = head
        .current
        .ok_or_else(|| Error::CurrentGroupKeyUnavailable {
            workspace_id: workspace_id.to_owned(),
        })?;
    let new_wrapped = crate::crypto::wrap_content_key_for_keyring(&content_key, current)?;

    let mut rewrapped = document.clone();
    rewrapped.encryption = records::Encryption::Keyring(records::KeyringEncryption {
        keyring_ref: records::KeyringRef {
            keyring: workspace_id.to_string(),
            wrapped_content_key: AtBytes {
                encoded: base64::engine::general_purpose::STANDARD.encode(&new_wrapped),
            },
            rotation: head.current_rotation,
        },
        algo: ke.algo.clone(),
        nonce: ke.nonce.clone(),
    });
    Ok(RewrapPlan::Rewrap(Box::new(rewrapped)))
}

/// Fetch one document, re-wrap its content key to the head rotation, and
/// write it back conditioned on the CID just read (compare-and-swap).
///
/// A CAS conflict returns [`RewrapItem::Conflict`], not an error: the record
/// changed under us — almost always another runner finished it first — so the
/// runner re-derives (finds nothing to do) and skips.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn rewrap_document_to_head<T: Transport>(
    client: &mut XrpcClient<T>,
    workspace_id: &str,
    doc_uri: &str,
    head: GroupKeys<'_>,
) -> Result<RewrapItem, Error> {
    let at_uri = atproto::parse_at_uri(doc_uri)?;
    let entry = client
        .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
        .await?;
    let cid = entry.cid;
    let document: records::Document = serde_json::from_value(entry.value)?;

    let rewrapped = match plan_rewrap(&document, workspace_id, head)? {
        RewrapPlan::NotApplicable => return Ok(RewrapItem::NotApplicable),
        RewrapPlan::AlreadyCurrent => return Ok(RewrapItem::AlreadyCurrent),
        RewrapPlan::Rewrap(doc) => doc,
    };

    match client
        .put_record_conditional(DOCUMENT_COLLECTION, &at_uri.rkey, &*rewrapped, Some(&cid))
        .await
    {
        Ok(_) => {
            trace!(
                "rewrap: migrated {doc_uri} to rotation {}",
                head.current_rotation
            );
            Ok(RewrapItem::Rewrapped)
        }
        Err(Error::CasConflict(_)) => {
            trace!("rewrap: CAS conflict on {doc_uri}, another runner won — skip");
            Ok(RewrapItem::Conflict)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
#[path = "rewrap_tests.rs"]
mod tests;
