use log::trace;
use serde::Serialize;

use super::{RecordEntry, RecordPage, RecordRef, Transport};
use crate::client::transport::*;
use crate::error::Error;

/// Inject `$type` into a serialized record value.
fn record_with_type<R: Serialize>(collection: &str, record: &R) -> serde_json::Value {
    let mut value = serde_json::to_value(record).expect("record must be serializable");
    if let serde_json::Value::Object(ref mut map) = value {
        map.insert("$type".into(), collection.into());
    }
    value
}

impl<T: Transport> super::XrpcClient<T> {
    /// Create a record via `com.atproto.repo.createRecord`.
    ///
    /// When `rkey` is `Some`, the PDS commits at the chosen key (used for
    /// stable identities like TID-derived document keys). When `None`, the
    /// PDS allocates a fresh rkey.
    pub async fn create_record<R: Serialize>(
        &mut self,
        collection: &str,
        rkey: Option<&str>,
        record: &R,
    ) -> Result<RecordRef, Error> {
        trace!("creating record in {}", collection);
        let did = self.did()?.to_owned();

        let mut body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "record": record_with_type(collection, record),
        });
        if let Some(rkey) = rkey {
            body["rkey"] = serde_json::Value::String(rkey.to_owned());
        }

        let mut request = HttpRequest {
            method: HttpMethod::Post,
            url: format!("{}/xrpc/com.atproto.repo.createRecord", self.base_url),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: Some(RequestBody::Json(body)),
        };
        self.attach_auth(&mut request)?;

        let response = self.send_checked(request).await?;
        Ok(serde_json::from_slice(&response.body)?)
    }

    /// Upsert a record with an explicit rkey via `com.atproto.repo.putRecord`.
    ///
    /// Idempotent — creates or overwrites the record at `collection/rkey`.
    /// Used for singleton records like `at.opake.publicKey/self`.
    pub async fn put_record<R: Serialize>(
        &mut self,
        collection: &str,
        rkey: &str,
        record: &R,
    ) -> Result<RecordRef, Error> {
        self.put_record_conditional(collection, rkey, record, None)
            .await
    }

    /// Upsert a record conditioned on its current CID via `putRecord`'s
    /// `swapRecord`. This is the per-record compare-and-swap primitive.
    ///
    /// `swap_record`:
    /// - `Some(cid)` — the write commits only if the record at
    ///   `collection/rkey` still has CID `cid`. If it moved, the PDS rejects
    ///   with [`Error::CasConflict`]: another writer got there first, and a
    ///   background runner should re-derive the item and skip.
    /// - `None` — an unconditional upsert (identical to [`Self::put_record`]).
    ///
    /// spec:background-work § Concurrency is resolved per record by compare-and-swap
    pub async fn put_record_conditional<R: Serialize>(
        &mut self,
        collection: &str,
        rkey: &str,
        record: &R,
        swap_record: Option<&str>,
    ) -> Result<RecordRef, Error> {
        trace!("putting record {}/{}", collection, rkey);
        let did = self.did()?.to_owned();

        let mut body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "rkey": rkey,
            "record": record_with_type(collection, record),
        });
        if let Some(cid) = swap_record {
            body["swapRecord"] = serde_json::Value::String(cid.to_owned());
        }

        let mut request = HttpRequest {
            method: HttpMethod::Post,
            url: format!("{}/xrpc/com.atproto.repo.putRecord", self.base_url),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: Some(RequestBody::Json(body)),
        };
        self.attach_auth(&mut request)?;

        let response = self.send_checked(request).await?;
        Ok(serde_json::from_slice(&response.body)?)
    }

    /// Fetch a single record via `com.atproto.repo.getRecord`.
    pub async fn get_record(
        &mut self,
        did: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<RecordEntry, Error> {
        trace!("getting record {}/{}/{}", did, collection, rkey);
        let url = format!(
            "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey={}",
            self.base_url, did, collection, rkey,
        );

        let mut request = HttpRequest {
            method: HttpMethod::Get,
            url,
            headers: vec![],
            body: None,
        };
        self.attach_auth(&mut request)?;

        let response = self.send_checked(request).await?;
        Ok(serde_json::from_slice(&response.body)?)
    }

    /// List records in a collection via `com.atproto.repo.listRecords`.
    pub async fn list_records(
        &mut self,
        collection: &str,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<RecordPage, Error> {
        trace!("listing records in {}", collection);
        let did = self.did()?.to_owned();

        let mut url = format!(
            "{}/xrpc/com.atproto.repo.listRecords?repo={}&collection={}",
            self.base_url, did, collection,
        );
        if let Some(limit) = limit {
            url.push_str(&format!("&limit={}", limit));
        }
        if let Some(cursor) = cursor {
            url.push_str(&format!("&cursor={}", cursor));
        }

        let mut request = HttpRequest {
            method: HttpMethod::Get,
            url,
            headers: vec![],
            body: None,
        };
        self.attach_auth(&mut request)?;

        let response = self.send_checked(request).await?;
        Ok(serde_json::from_slice(&response.body)?)
    }

    /// Delete a record via `com.atproto.repo.deleteRecord`.
    pub async fn delete_record(&mut self, collection: &str, rkey: &str) -> Result<(), Error> {
        self.delete_record_conditional(collection, rkey, None).await
    }

    /// Delete a record conditioned on its current CID via `deleteRecord`'s
    /// `swapRecord`. The delete-side compare-and-swap primitive.
    ///
    /// `swap_record`:
    /// - `Some(cid)` — the delete commits only if the record still has CID
    ///   `cid`; otherwise the PDS rejects with [`Error::CasConflict`].
    /// - `None` — an unconditional delete (identical to [`Self::delete_record`]).
    ///
    /// spec:background-work § Concurrency is resolved per record by compare-and-swap
    pub async fn delete_record_conditional(
        &mut self,
        collection: &str,
        rkey: &str,
        swap_record: Option<&str>,
    ) -> Result<(), Error> {
        trace!("deleting record {}/{}", collection, rkey);
        let did = self.did()?.to_owned();

        let mut body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "rkey": rkey,
        });
        if let Some(cid) = swap_record {
            body["swapRecord"] = serde_json::Value::String(cid.to_owned());
        }

        let mut request = HttpRequest {
            method: HttpMethod::Post,
            url: format!("{}/xrpc/com.atproto.repo.deleteRecord", self.base_url),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: Some(RequestBody::Json(body)),
        };
        self.attach_auth(&mut request)?;

        self.send_checked(request).await?;
        Ok(())
    }

    /// Execute multiple record operations atomically via `com.atproto.repo.applyWrites`.
    ///
    /// All writes succeed or fail together. Useful for directory moves
    /// (remove from source + add to target in one atomic call). Discards
    /// the per-write results — use [`Self::apply_writes_returning`] if
    /// you need the URIs/CIDs of created records (e.g. for chain writes).
    pub async fn apply_writes(&mut self, writes: &[ApplyWriteOp]) -> Result<(), Error> {
        let _ = self.apply_writes_returning(writes).await?;
        Ok(())
    }

    /// Like [`Self::apply_writes`] but returns the result of each write.
    ///
    /// Result order matches `writes` order. `Create` ops carry the
    /// assigned URI + CID; `Update` ops carry the URI + new CID;
    /// `Delete` ops have all-`None` fields.
    pub async fn apply_writes_returning(
        &mut self,
        writes: &[ApplyWriteOp],
    ) -> Result<Vec<ApplyWriteResult>, Error> {
        trace!("applying {} writes atomically", writes.len());
        let did = self.did()?.to_owned();

        let ops: Vec<serde_json::Value> = writes
            .iter()
            .map(|op| match op {
                ApplyWriteOp::Create {
                    collection,
                    rkey,
                    record,
                } => {
                    let mut op = serde_json::json!({
                        "$type": "com.atproto.repo.applyWrites#create",
                        "collection": collection,
                        "value": ApplyWriteOp::typed_value(collection, record),
                    });
                    if let Some(rkey) = rkey {
                        op["rkey"] = serde_json::Value::String(rkey.clone());
                    }
                    op
                }
                ApplyWriteOp::Update {
                    collection,
                    rkey,
                    record,
                } => serde_json::json!({
                    "$type": "com.atproto.repo.applyWrites#update",
                    "collection": collection,
                    "rkey": rkey,
                    "value": ApplyWriteOp::typed_value(collection, record),
                }),
                ApplyWriteOp::Delete { collection, rkey } => serde_json::json!({
                    "$type": "com.atproto.repo.applyWrites#delete",
                    "collection": collection,
                    "rkey": rkey,
                }),
            })
            .collect();

        let body = serde_json::json!({
            "repo": did,
            "writes": ops,
        });

        let mut request = HttpRequest {
            method: HttpMethod::Post,
            url: format!("{}/xrpc/com.atproto.repo.applyWrites", self.base_url),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: Some(RequestBody::Json(body)),
        };
        self.attach_auth(&mut request)?;

        let response = self.send_checked(request).await?;
        // The applyWrites response carries a `results` array with one
        // entry per write. Older PDS versions returned no body — treat
        // that as an empty results vec; callers depending on returned
        // CIDs will then fail with a clear error rather than misread.
        if response.body.is_empty() {
            return Ok(Vec::new());
        }
        #[derive(serde::Deserialize)]
        struct ApplyWritesResponse {
            #[serde(default)]
            results: Vec<ApplyWriteResult>,
        }
        let parsed: ApplyWritesResponse = serde_json::from_slice(&response.body)?;
        Ok(parsed.results)
    }
}

/// One write's result from `applyWrites`. Field presence depends on the
/// op type — `Create` carries `uri` + `cid`; `Delete` carries neither.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ApplyWriteResult {
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub cid: Option<String>,
}

/// A single operation for [`XrpcClient::apply_writes`].
///
/// `Create` and `Update` auto-inject `$type` from the collection name,
/// matching the behavior of `create_record` and `put_record`.
pub enum ApplyWriteOp {
    Create {
        collection: String,
        /// Explicit rkey. If `None`, the PDS generates one (TID).
        /// Set this when the URI must be known before sending (e.g. for
        /// atomic directory placement).
        rkey: Option<String>,
        record: serde_json::Value,
    },
    Update {
        collection: String,
        rkey: String,
        record: serde_json::Value,
    },
    Delete {
        collection: String,
        rkey: String,
    },
}

impl ApplyWriteOp {
    /// Inject `$type` into a record value, matching `create_record`'s behavior.
    fn typed_value(collection: &str, record: &serde_json::Value) -> serde_json::Value {
        let mut value = record.clone();
        if let serde_json::Value::Object(ref mut map) = value {
            map.insert("$type".into(), collection.into());
        }
        value
    }
}
