use log::debug;
use serde::Deserialize;

use super::Transport;
use crate::atproto::BlobRef;
use crate::client::transport::*;
use crate::error::Error;

impl<T: Transport> super::XrpcClient<T> {
    /// Upload raw bytes as a blob via `com.atproto.repo.uploadBlob`.
    pub async fn upload_blob(&mut self, data: Vec<u8>, mime_type: &str) -> Result<BlobRef, Error> {
        debug!("uploading blob ({} bytes, {})", data.len(), mime_type);

        let mut request = HttpRequest {
            method: HttpMethod::Post,
            url: format!("{}/xrpc/com.atproto.repo.uploadBlob", self.base_url),
            headers: vec![("Content-Type".into(), mime_type.into())],
            body: Some(RequestBody::Bytes {
                data,
                content_type: mime_type.into(),
            }),
        };
        self.attach_auth(&mut request)?;

        let response = self.send_checked(request).await?;

        #[derive(Deserialize)]
        struct UploadResponse {
            blob: BlobRef,
        }

        let parsed: UploadResponse = serde_json::from_slice(&response.body)?;
        Ok(parsed.blob)
    }

    /// Fetch a blob by DID + CID via `com.atproto.sync.getBlob`.
    pub async fn get_blob(&mut self, did: &str, cid: &str) -> Result<Vec<u8>, Error> {
        debug!("fetching blob did={} cid={}", did, cid);
        let url = format!(
            "{}/xrpc/com.atproto.sync.getBlob?did={}&cid={}",
            self.base_url, did, cid,
        );

        let mut request = HttpRequest {
            method: HttpMethod::Get,
            url,
            headers: vec![],
            body: None,
        };
        self.attach_auth(&mut request)?;

        let response = self.send_checked(request).await?;
        Ok(response.body)
    }
}
