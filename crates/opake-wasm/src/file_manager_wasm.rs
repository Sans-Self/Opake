// WasmFileManagerHandle — file operations within a cabinet or workspace.
//
// Holds an Rc clone of the parent OpakeContext's async Mutex over the
// same WasmOpake. All async methods lock the Mutex for the duration of
// the operation; concurrent calls (e.g., upload in-flight + token
// refresh) queue rather than panic.
//
// Methods take &self (not &mut self) — the Mutex provides interior
// mutability, avoiding wasm-bindgen's borrow tracking which panics
// on concurrent async &mut self operations.

use std::rc::Rc;

use futures_util::lock::Mutex;
use opake_core::indexer::tree_keeper::TreeKeeper;
use opake_core::manager::{FileContext, UploadRequest};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::opake_wasm::OpakeGuard;
use crate::wasm_util::{
    build_snapshot, pub_key_from_slice, to_js, wasm_err, DownloadResult, MutationResultDto,
    WasmOpake,
};

/// WASM FileManager handle.
#[wasm_bindgen(js_name = FileManager)]
pub struct WasmFileManagerHandle {
    pub(crate) opake: Rc<Mutex<WasmOpake>>,
    /// Shared TreeKeeper cloned from the parent OpakeContext. Used for
    /// SSE-driven watcher registration.
    pub(crate) tree_keeper: Rc<Mutex<TreeKeeper>>,
    pub(crate) context: FileContext,
}

#[wasm_bindgen(js_class = FileManager)]
impl WasmFileManagerHandle {
    pub async fn upload(
        &self,
        plaintext: &[u8],
        filename: &str,
        mime_type: &str,
        description: Option<String>,
        tags: JsValue,
        directory_uri: Option<String>,
    ) -> Result<JsValue, JsError> {
        let tags_vec: Vec<String> = if tags.is_null() || tags.is_undefined() {
            vec![]
        } else {
            serde_wasm_bindgen::from_value(tags).map_err(|e| JsError::new(&e.to_string()))?
        };

        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let result = mgr
            .upload(&UploadRequest {
                plaintext,
                filename,
                mime_type,
                description: description.as_deref(),
                tags: &tags_vec,
                directory_uri: directory_uri.as_deref(),
            })
            .await
            .map_err(wasm_err)?;

        to_js(&MutationResultDto {
            uri: Some(result.uri),
            proposed: result.outcome.is_proposed(),
        })
    }

    pub async fn download(&self, document_uri: &str) -> Result<JsValue, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let result = mgr.download(document_uri).await.map_err(wasm_err)?;

        to_js(&DownloadResult {
            filename: result.filename,
            plaintext: result.plaintext,
        })
    }

    pub async fn delete(
        &self,
        document_uri: &str,
        parent_directory_uri: &str,
    ) -> Result<JsValue, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let result = mgr
            .delete(document_uri, parent_directory_uri)
            .await
            .map_err(wasm_err)?;
        to_js(&MutationResultDto {
            uri: None,
            proposed: result.is_proposed(),
        })
    }

    #[wasm_bindgen(js_name = moveEntry)]
    pub async fn move_entry(
        &self,
        entry_uri: &str,
        source_dir: &str,
        target_dir: &str,
    ) -> Result<JsValue, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let result = mgr
            .move_entry(entry_uri, source_dir, target_dir)
            .await
            .map_err(wasm_err)?;
        to_js(&MutationResultDto {
            uri: None,
            proposed: result.is_proposed(),
        })
    }

    #[wasm_bindgen(js_name = createDirectory)]
    pub async fn create_directory(
        &self,
        name: &str,
        parent_uri: Option<String>,
    ) -> Result<JsValue, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let result = mgr
            .create_directory(name, parent_uri.as_deref())
            .await
            .map_err(wasm_err)?;
        to_js(&MutationResultDto {
            uri: Some(result.uri),
            proposed: result.outcome.is_proposed(),
        })
    }

    #[wasm_bindgen(js_name = ensureRoot)]
    pub async fn ensure_root(&self) -> Result<String, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        mgr.ensure_root().await.map_err(wasm_err)
    }

    /// Load the directory tree (read-only, no PDS writes).
    #[wasm_bindgen(js_name = loadTree)]
    pub async fn load_tree(&self) -> Result<JsValue, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let tree = mgr.load_tree().await.map_err(wasm_err)?;
        let snapshot = build_snapshot(&tree);
        to_js(&serde_json::json!({ "snapshot": snapshot }))
    }

    /// Load tree + metadata (read-only, no proposal application).
    #[wasm_bindgen(js_name = loadTreeWithMetadata)]
    pub async fn load_tree_with_metadata(
        &self,
        metadata_for_dir: Option<String>,
    ) -> Result<JsValue, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let tree = mgr.load_tree().await.map_err(wasm_err)?;
        let snapshot = build_snapshot(&tree);

        let metadata = if let Some(ref dir_uri) = metadata_for_dir {
            if dir_uri == "*" {
                let mut all = std::collections::HashMap::new();
                for uri in tree.all_directory_uris() {
                    match mgr.resolve_document_metadata_in(&tree, uri).await {
                        Ok(m) => all.extend(m),
                        Err(e) => log::warn!("metadata resolution failed for {uri}: {e}"),
                    }
                }
                Some(all)
            } else {
                let target = if dir_uri.is_empty() {
                    tree.root_uri().unwrap_or(dir_uri)
                } else {
                    dir_uri.as_str()
                };
                Some(
                    mgr.resolve_document_metadata_in(&tree, target)
                        .await
                        .map_err(wasm_err)?,
                )
            }
        } else {
            None
        };

        to_js(&serde_json::json!({
            "snapshot": snapshot,
            "metadata": metadata,
        }))
    }

    /// Load tree, apply proposals, resolve metadata — the full sync cycle.
    #[wasm_bindgen(js_name = syncAndLoadTree)]
    pub async fn sync_and_load_tree(
        &self,
        metadata_for_dir: Option<String>,
    ) -> Result<JsValue, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let tree = mgr.load_tree().await.map_err(wasm_err)?;
        let snapshot = build_snapshot(&tree);

        let mut metadata = if let Some(ref dir_uri) = metadata_for_dir {
            if dir_uri == "*" {
                let mut all = std::collections::HashMap::new();
                for uri in tree.all_directory_uris() {
                    match mgr.resolve_document_metadata_in(&tree, uri).await {
                        Ok(m) => all.extend(m),
                        Err(e) => log::warn!("metadata resolution failed for {uri}: {e}"),
                    }
                }
                Some(all)
            } else {
                let target = if dir_uri.is_empty() {
                    tree.root_uri().unwrap_or(dir_uri)
                } else {
                    dir_uri.as_str()
                };
                let m = mgr
                    .resolve_document_metadata_in(&tree, target)
                    .await
                    .map_err(wasm_err)?;
                Some(m)
            }
        } else {
            None
        };

        mgr.cleanup_own_applied_proposals(&tree).await;
        let applied = mgr.apply_pending_proposals(&tree).await.unwrap_or(0);
        let proposals = mgr.proposals().to_vec();

        let proposal_uris: Vec<&str> = proposals
            .iter()
            .filter_map(|p| p.entry_uri.as_deref())
            .collect();
        if !proposal_uris.is_empty() {
            if let Ok(proposal_meta) = mgr.resolve_document_metadata_for(&proposal_uris).await {
                metadata
                    .get_or_insert_with(std::collections::HashMap::new)
                    .extend(proposal_meta);
            }
        }

        to_js(&serde_json::json!({
            "snapshot": snapshot,
            "metadata": metadata,
            "proposals": proposals,
            "proposals_applied": applied,
        }))
    }

    /// Apply pending proposals without metadata resolution (daemon use).
    #[wasm_bindgen(js_name = syncAndApplyProposals)]
    pub async fn sync_and_apply_proposals(&self) -> Result<usize, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let tree = mgr.load_tree().await.map_err(wasm_err)?;
        mgr.apply_pending_proposals(&tree).await.map_err(wasm_err)
    }

    // -- Editor operations --

    #[wasm_bindgen(js_name = renameDirectory)]
    pub async fn rename_directory(
        &self,
        directory_uri: &str,
        new_name: &str,
    ) -> Result<JsValue, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let result = mgr
            .rename_directory(directory_uri, new_name)
            .await
            .map_err(wasm_err)?;
        to_js(&MutationResultDto {
            uri: None,
            proposed: result.is_proposed(),
        })
    }

    #[wasm_bindgen(js_name = updateMetadata)]
    pub async fn update_metadata(
        &self,
        document_uri: &str,
        name: Option<String>,
        tags: JsValue,
        description: Option<String>,
    ) -> Result<JsValue, JsError> {
        let tags_vec: Option<Vec<String>> = if tags.is_null() || tags.is_undefined() {
            None
        } else {
            Some(serde_wasm_bindgen::from_value(tags).map_err(|e| JsError::new(&e.to_string()))?)
        };

        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let metadata = mgr
            .update_metadata(document_uri, |meta| {
                if let Some(ref n) = name {
                    meta.name = n.clone();
                }
                if let Some(ref t) = tags_vec {
                    meta.tags = t.clone();
                }
                if let Some(ref d) = description {
                    meta.description = Some(d.clone());
                }
            })
            .await
            .map_err(wasm_err)?;

        to_js(&metadata)
    }

    #[wasm_bindgen(js_name = updateContent)]
    pub async fn update_content(
        &self,
        document_uri: &str,
        new_plaintext: &[u8],
    ) -> Result<String, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        mgr.update_content(document_uri, new_plaintext)
            .await
            .map_err(wasm_err)
    }

    // -- Sharing --

    pub async fn share(
        &self,
        document_uri: &str,
        recipient_did: &str,
        recipient_public_key: &[u8],
        permissions: &str,
        note: Option<String>,
    ) -> Result<String, JsError> {
        let pubkey = pub_key_from_slice(recipient_public_key)?;
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        mgr.share(
            document_uri,
            recipient_did,
            &pubkey,
            permissions,
            note.as_deref(),
        )
        .await
        .map_err(wasm_err)
    }

    #[wasm_bindgen(js_name = revokeShare)]
    pub async fn revoke_share(&self, grant_uri: &str) -> Result<(), JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        mgr.revoke_share(grant_uri).await.map_err(wasm_err)
    }

    #[wasm_bindgen(js_name = listShares)]
    pub async fn list_shares(&self) -> Result<JsValue, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let shares = mgr.list_shares().await.map_err(wasm_err)?;
        to_js(&shares)
    }

    #[wasm_bindgen(js_name = createPendingShare)]
    pub async fn create_pending_share(
        &self,
        document_uri: &str,
        recipient: &str,
        permissions: &str,
        note: Option<String>,
    ) -> Result<String, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        mgr.create_pending_share(document_uri, recipient, permissions, note.as_deref())
            .await
            .map_err(wasm_err)
    }

    #[wasm_bindgen(js_name = deleteRecursive)]
    pub async fn delete_recursive(&self, uri: &str) -> Result<JsValue, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let tree = mgr.load_tree().await.map_err(wasm_err)?;
        let resolved = mgr.resolve_entry(&tree, uri).await.map_err(wasm_err)?;
        let result = mgr
            .delete_recursive(&tree, &resolved, true)
            .await
            .map_err(wasm_err)?;

        #[derive(Serialize)]
        struct R {
            documents_deleted: usize,
            directories_deleted: usize,
        }
        to_js(&R {
            documents_deleted: result.documents_deleted,
            directories_deleted: result.directories_deleted,
        })
    }

    /// Resolve document metadata within a directory for a specific set of URIs.
    #[wasm_bindgen(js_name = resolveDocumentMetadataIn)]
    pub async fn resolve_document_metadata_in(
        &self,
        directory_uri: &str,
    ) -> Result<JsValue, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let mut mgr = opake.file_manager(ctx);
        let tree = mgr.load_tree().await.map_err(wasm_err)?;
        let metadata = mgr
            .resolve_document_metadata_in(&tree, directory_uri)
            .await
            .map_err(wasm_err)?;
        to_js(&metadata)
    }

    /// Fetch and decrypt metadata for a single document by URI.
    #[wasm_bindgen(js_name = getDocumentMetadata)]
    pub async fn get_document_metadata(&self, document_uri: &str) -> Result<JsValue, JsError> {
        let (mut opake, ctx) = self.parts().await?;
        let did = opake.did().to_owned();
        let private_key = opake.identity().x25519_private_key_bytes().map_err(wasm_err)?;
        let group_key = match ctx {
            FileContext::Workspace(ref ws) => Some(ws.key.clone()),
            FileContext::Cabinet(_) => None,
        };
        let result = opake_core::metadata::fetch_document_metadata(
            opake.client_mut(),
            document_uri,
            &did,
            &private_key,
            group_key.as_ref(),
        )
        .await
        .map_err(wasm_err)?;
        let resolved = opake_core::manager::ResolvedDocumentMetadata::from_parts(
            result.metadata,
            result.document.created_at,
            result.document.modified_at,
        );
        to_js(&resolved)
    }

    #[wasm_bindgen(js_name = isOwner)]
    pub fn is_owner(&self) -> Result<bool, JsError> {
        let guard = self
            .opake
            .try_lock()
            .ok_or_else(|| JsError::new("Opake is busy — an operation is in progress"))?;
        Ok(guard.did() == self.context.owner_did())
    }

    /// Lock the Mutex and return the Opake + FileContext.
    async fn parts(&self) -> Result<(OpakeGuard<'_>, &FileContext), JsError> {
        Ok((self.opake.lock().await, &self.context))
    }
}
