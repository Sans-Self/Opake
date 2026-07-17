use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use super::{CidLink, EncryptedMetadata, KeyWrapping, SCHEMA_VERSION};

/// One entry in a directory's listing.
///
/// The CID pins the version of the target record observed at the moment this
/// directory was written. For child directories it points at the head of that
/// path's chain at write time; for documents it points at the document record
/// itself. Indexers use these CIDs to detect concurrent supersedes that touch
/// the same child path without re-fetching every target.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingEntry {
    pub target: String,
    pub target_cid: CidLink,
}

impl ListingEntry {
    pub fn new(target: impl Into<String>, target_cid: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            target_cid: CidLink {
                cid: target_cid.into(),
            },
        }
    }

    /// Whether this entry's `target_cid` is the empty-string sentinel left
    /// behind when we hydrate a pre-Phase-1 legacy entry (see
    /// [`ListingEntry`]'s `Deserialize` impl).
    pub fn has_legacy_empty_cid(&self) -> bool {
        self.target_cid.cid.is_empty()
    }
}

/// Pull a target AT-URI out of a wire-shape entry value. Accepts both
/// the modern `{target, targetCid}` form and the pre-Phase-1 bare-URI
/// string form. Returns `None` on shapes we don't recognise (caller
/// usually filters those out silently).
pub fn entry_target_uri(value: &serde_json::Value) -> Option<&str> {
    match value {
        serde_json::Value::String(s) => Some(s.as_str()),
        serde_json::Value::Object(map) => map.get("target").and_then(|v| v.as_str()),
        _ => None,
    }
}

/// Custom deserializer that accepts both the current `{target, targetCid}`
/// shape and the pre-Phase-1 legacy shape — a bare AT-URI string. Legacy
/// entries hydrate with `target_cid: ""` as a sentinel.
///
/// Why this exists: Phase 1 dropped the `[at-uri]` shape in favour of
/// `[{target, targetCid}]`. Workspaces were wiped at the same time so the
/// new shape is the only one present in workspace records; cabinet records
/// on user PDSes were never explicitly wiped though, and re-deserializing
/// an old cabinet root would otherwise fail with a hard type error during
/// the first add-entry call.
///
/// The empty-CID sentinel is safe for cabinet flows (they don't propagate
/// CIDs into anything) and is unreachable for workspace flows (no legacy
/// workspace data exists). Indexer-side authority validation lives over
/// the federation cascade, which cabinet records never enter.
impl<'de> Deserialize<'de> for ListingEntry {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Modern {
                target: String,
                #[serde(rename = "targetCid")]
                target_cid: CidLink,
            },
            Legacy(String),
        }

        match Wire::deserialize(de).map_err(de::Error::custom)? {
            Wire::Modern { target, target_cid } => Ok(ListingEntry { target, target_cid }),
            Wire::Legacy(target) => {
                log::warn!(
                    "ListingEntry: hydrating pre-Phase-1 legacy entry {target} \
                     with empty targetCid sentinel"
                );
                Ok(ListingEntry {
                    target,
                    target_cid: CidLink { cid: String::new() },
                })
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    pub opake_version: u32,
    pub key_wrapping: KeyWrapping,
    pub encrypted_metadata: EncryptedMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<ListingEntry>,
    /// AT-URI of the prior canonical directory at this path, if any. Absent
    /// on the genesis record of a chain. Indexers walk this back-edge to
    /// verify the chain and detect forks (two records superseding the same
    /// prior URI).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    /// CID of the exact predecessor record named by `supersedes`. Present
    /// whenever `supersedes` is; readers verify a fetched predecessor's bytes
    /// against it. Each cascade level pins its own predecessor, never copied
    /// through.
    // spec: lineage § Supersede references carry a content pin
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes_cid: Option<String>,
    /// This directory chain's genesis URI — the directory's stable object
    /// identity. Absent on a genesis directory (and always absent on
    /// cabinet directories, which never supersede), present and never
    /// changing on every supersede. Metadata ciphertexts are AEAD-bound to
    /// the anchor this resolves to, which is how a ciphertext copied
    /// verbatim through a cascade still authenticates.
    // spec: lineage § Lineage is the chain's genesis URI, carried on every supersede
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage: Option<String>,
    /// Genesis keyring URI of the workspace this directory belongs to.
    /// Absent for cabinet directories. Carried explicitly so any reader
    /// can resolve workspace identity without walking the keyring chain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// True if this directory is part of the workspace-root chain. Stamped
    /// on every record in the root chain (genesis + every supersede). Replaces
    /// the prior deterministic `ws-{keyringRkey}` rkey convention — with this
    /// marker, the workspace root can be TID-rkeyed and freely deleted /
    /// recreated. Absent (defaults false) on subdirectories and cabinet
    /// directories. Indexer enforces: only managers may set it, the value
    /// never flips across a supersede, and each workspace has at most one
    /// active root chain.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_workspace_root: bool,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

#[inline]
fn is_false(value: &bool) -> bool {
    !*value
}

impl Directory {
    pub fn new(
        key_wrapping: KeyWrapping,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            key_wrapping,
            encrypted_metadata,
            entries: Vec::new(),
            supersedes: None,
            supersedes_cid: None,
            lineage: None,
            workspace_id: None,
            is_workspace_root: false,
            created_at,
            modified_at: None,
        }
    }

    /// Stamp the workspace's genesis keyring URI onto this record. Builder-
    /// style so callers can chain after `new`.
    pub fn with_workspace_id(mut self, workspace_id: impl Into<String>) -> Self {
        self.workspace_id = Some(workspace_id.into());
        self
    }

    /// Mark this directory as part of the workspace-root chain. Writers stamp
    /// this on the genesis root and every subsequent supersede in the root
    /// chain. Builder-style.
    pub fn as_workspace_root(mut self) -> Self {
        self.is_workspace_root = true;
        self
    }

    /// Stamp the directory chain's genesis URI onto a supersede record.
    /// Genesis records leave `lineage` absent — they identify themselves.
    pub fn with_lineage(mut self, lineage: impl Into<String>) -> Self {
        self.lineage = Some(lineage.into());
        self
    }

    /// Pin the CID of the immediate predecessor this record supersedes.
    pub fn with_supersedes_cid(mut self, cid: impl Into<String>) -> Self {
        self.supersedes_cid = Some(cid.into());
        self
    }

    /// The lineage anchor: the chain's genesis URI, which this directory's
    /// metadata ciphertext is AEAD-bound to. The declared `lineage` once
    /// the directory has been superseded at least once, or the record's
    /// own URI on a genesis (or cabinet) directory.
    // spec: lineage § Lineage is the chain's genesis URI, carried on every supersede
    pub fn lineage_anchor<'a>(&'a self, self_uri: &'a str) -> &'a str {
        self.lineage.as_deref().unwrap_or(self_uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserializes_modern_listing_entry() {
        let value = json!({
            "target": "at://did:plc:abc/at.opake.document/3kkk",
            "targetCid": { "$link": "bafyabc" },
        });
        let entry: ListingEntry = serde_json::from_value(value).unwrap();
        assert_eq!(entry.target, "at://did:plc:abc/at.opake.document/3kkk");
        assert_eq!(entry.target_cid.cid, "bafyabc");
        assert!(!entry.has_legacy_empty_cid());
    }

    #[test]
    fn deserializes_legacy_string_listing_entry() {
        let value = json!("at://did:plc:abc/at.opake.document/3kkk");
        let entry: ListingEntry = serde_json::from_value(value).unwrap();
        assert_eq!(entry.target, "at://did:plc:abc/at.opake.document/3kkk");
        assert!(entry.has_legacy_empty_cid());
    }

    #[test]
    fn deserializes_mixed_legacy_and_modern_entries_in_vec() {
        // The realistic worst case: an old cabinet root that gets a new
        // entry appended through the modern code path and re-uploaded —
        // future reads see a mixed listing. Both rows must survive.
        let value = json!([
            "at://did:plc:abc/at.opake.document/legacy1",
            {
                "target": "at://did:plc:abc/at.opake.document/modern1",
                "targetCid": { "$link": "bafymodern" }
            }
        ]);
        let entries: Vec<ListingEntry> = serde_json::from_value(value).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].has_legacy_empty_cid());
        assert_eq!(
            entries[0].target,
            "at://did:plc:abc/at.opake.document/legacy1"
        );
        assert!(!entries[1].has_legacy_empty_cid());
        assert_eq!(entries[1].target_cid.cid, "bafymodern");
    }
}
