// reqwest-based Transport implementation for native (non-WASM) targets.

use opake_core::client::{HttpMethod, HttpRequest, HttpResponse, RequestBody, Transport};
use opake_core::error::Error;

pub struct ReqwestTransport {
    http: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }
}

impl Transport for ReqwestTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, Error> {
        let mut builder = match request.method {
            HttpMethod::Get => self.http.get(&request.url),
            HttpMethod::Post => self.http.post(&request.url),
        };

        for (key, value) in &request.headers {
            builder = builder.header(key, value);
        }

        if let Some(body) = request.body {
            builder = match body {
                RequestBody::Json(json) => builder.json(&json),
                RequestBody::Bytes { data, content_type } => {
                    builder.header("Content-Type", content_type).body(data)
                }
                RequestBody::Form(params) => {
                    let encoded: String = params
                        .iter()
                        .map(|(k, v)| {
                            format!("{}={}", urlencoding::encode(k), urlencoding::encode(v))
                        })
                        .collect::<Vec<_>>()
                        .join("&");
                    builder
                        .header("Content-Type", "application/x-www-form-urlencoded")
                        .body(encoded)
                }
            };
        }

        let response = builder.send().await.map_err(|e| Error::Xrpc {
            status: e.status().map(|s| s.as_u16()).unwrap_or(0),
            message: e.to_string(),
        })?;

        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_owned(), v.to_str().unwrap_or("").to_owned()))
            .collect();
        let body = response.bytes().await.map_err(|e| Error::Xrpc {
            status,
            message: e.to_string(),
        })?;

        Ok(HttpResponse {
            status,
            headers,
            body: body.to_vec(),
        })
    }
}
