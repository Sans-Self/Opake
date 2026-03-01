use log::debug;
use serde::Serialize;

use super::{RecordEntry, RecordPage, RecordRef, Transport};
use crate::client::transport::*;
use crate::error::Error;

impl<T: Transport> super::XrpcClient<T> {
    /// Create a record via `com.atproto.repo.createRecord`.
    pub async fn create_record<R: Serialize>(
        &mut self,
        collection: &str,
        record: &R,
    ) -> Result<RecordRef, Error> {
        debug!("creating record in {}", collection);
        let auth = self.auth_header()?;
        let did = self.did()?;

        let body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "record": record,
        });

        let response = self
            .send_checked(HttpRequest {
                method: HttpMethod::Post,
                url: format!("{}/xrpc/com.atproto.repo.createRecord", self.base_url),
                headers: vec![auth, ("Content-Type".into(), "application/json".into())],
                body: Some(RequestBody::Json(body)),
            })
            .await?;

        Ok(serde_json::from_slice(&response.body)?)
    }

    /// Upsert a record with an explicit rkey via `com.atproto.repo.putRecord`.
    ///
    /// Idempotent — creates or overwrites the record at `collection/rkey`.
    /// Used for singleton records like `app.opake.cloud.publicKey/self`.
    pub async fn put_record<R: Serialize>(
        &mut self,
        collection: &str,
        rkey: &str,
        record: &R,
    ) -> Result<RecordRef, Error> {
        debug!("putting record {}/{}", collection, rkey);
        let auth = self.auth_header()?;
        let did = self.did()?;

        let body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "rkey": rkey,
            "record": record,
        });

        let response = self
            .send_checked(HttpRequest {
                method: HttpMethod::Post,
                url: format!("{}/xrpc/com.atproto.repo.putRecord", self.base_url),
                headers: vec![auth, ("Content-Type".into(), "application/json".into())],
                body: Some(RequestBody::Json(body)),
            })
            .await?;

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
        let auth = self.auth_header()?;
        let url = format!(
            "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey={}",
            self.base_url, did, collection, rkey,
        );

        let response = self
            .send_checked(HttpRequest {
                method: HttpMethod::Get,
                url,
                headers: vec![auth],
                body: None,
            })
            .await?;

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
        let auth = self.auth_header()?;
        let did = self.did()?;

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

        let response = self
            .send_checked(HttpRequest {
                method: HttpMethod::Get,
                url,
                headers: vec![auth],
                body: None,
            })
            .await?;

        Ok(serde_json::from_slice(&response.body)?)
    }

    /// Delete a record via `com.atproto.repo.deleteRecord`.
    pub async fn delete_record(&mut self, collection: &str, rkey: &str) -> Result<(), Error> {
        debug!("deleting record {}/{}", collection, rkey);
        let auth = self.auth_header()?;
        let did = self.did()?;

        let body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "rkey": rkey,
        });

        self.send_checked(HttpRequest {
            method: HttpMethod::Post,
            url: format!("{}/xrpc/com.atproto.repo.deleteRecord", self.base_url),
            headers: vec![auth, ("Content-Type".into(), "application/json".into())],
            body: Some(RequestBody::Json(body)),
        })
        .await?;

        Ok(())
    }
}
