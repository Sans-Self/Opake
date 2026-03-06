use log::debug;
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
    pub async fn create_record<R: Serialize>(
        &mut self,
        collection: &str,
        record: &R,
    ) -> Result<RecordRef, Error> {
        debug!("creating record in {}", collection);
        let did = self.did()?.to_owned();

        let body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "record": record_with_type(collection, record),
        });

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
    /// Used for singleton records like `app.opake.publicKey/self`.
    pub async fn put_record<R: Serialize>(
        &mut self,
        collection: &str,
        rkey: &str,
        record: &R,
    ) -> Result<RecordRef, Error> {
        debug!("putting record {}/{}", collection, rkey);
        let did = self.did()?.to_owned();

        let body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "rkey": rkey,
            "record": record_with_type(collection, record),
        });

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
        debug!("getting record {}/{}/{}", did, collection, rkey);
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
        debug!("listing records in {}", collection);
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
        debug!("deleting record {}/{}", collection, rkey);
        let did = self.did()?.to_owned();

        let body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "rkey": rkey,
        });

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
}
