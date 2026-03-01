// Transport trait — the injectable I/O boundary.
//
// This is the only thing a platform needs to provide: send an HTTP request,
// get bytes back. The CLI implements this with reqwest, the SPA with browser
// fetch via web_sys. Everything else (XRPC protocol, auth, response parsing)
// is built on top of this trait.

use crate::error::Error;

#[derive(Debug, Clone)]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone)]
pub enum RequestBody {
    Json(serde_json::Value),
    Bytes { data: Vec<u8>, content_type: String },
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<RequestBody>,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// The only thing a platform needs to provide: send an HTTP request, get bytes back.
/// CLI implements this with reqwest, the SPA with browser fetch via web_sys.
pub trait Transport {
    fn send(
        &self,
        request: HttpRequest,
    ) -> impl std::future::Future<Output = Result<HttpResponse, Error>>;
}
