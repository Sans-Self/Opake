// WasmTransport — Transport impl using the Fetch API via web_sys.
//
// Works in both Window and Web Worker contexts (uses js_sys::global()
// to avoid assuming either). Intended for the opake-wasm WASM module
// running inside a Comlink Web Worker.

use log::warn;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response};

use super::transport::{HttpMethod, HttpRequest, HttpResponse, RequestBody, Transport};
use crate::error::Error;

#[derive(Clone, Default)]
pub struct WasmTransport;

impl WasmTransport {
    pub fn new() -> Self {
        Self
    }
}

impl Transport for WasmTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, Error> {
        let method_str = match request.method {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
        };
        log::trace!("[WasmTransport] {} {}", method_str, &request.url);
        let mut opts = RequestInit::new();
        opts.method(match request.method {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
        });

        let headers = Headers::new().map_err(js_err)?;
        for (key, value) in &request.headers {
            // Skip Content-Type — body match arms below own it.
            if key.eq_ignore_ascii_case("content-type") {
                continue;
            }
            headers.set(key, value).map_err(js_err)?;
        }
        opts.headers(&headers);

        if let Some(body) = request.body {
            match body {
                RequestBody::Json(json) => {
                    let serialized = serde_json::to_string(&json)?;
                    headers
                        .set("Content-Type", "application/json")
                        .map_err(js_err)?;
                    opts.body(Some(&wasm_bindgen::JsValue::from_str(&serialized)));
                }
                RequestBody::Bytes { data, content_type } => {
                    headers.set("Content-Type", &content_type).map_err(js_err)?;
                    let array = js_sys::Uint8Array::from(data.as_slice());
                    opts.body(Some(&array));
                }
                RequestBody::Form(ref params) => {
                    let encoded = RequestBody::encode_form(params);
                    headers
                        .set("Content-Type", "application/x-www-form-urlencoded")
                        .map_err(js_err)?;
                    opts.body(Some(&wasm_bindgen::JsValue::from_str(&encoded)));
                }
            }
        }

        let req = Request::new_with_str_and_init(&request.url, &opts).map_err(js_err)?;

        // Use global fetch — works in both Window and Worker contexts.
        let global = js_sys::global();
        let promise = js_sys::Reflect::get(&global, &wasm_bindgen::JsValue::from_str("fetch"))
            .map_err(js_err)?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| Error::Xrpc {
                status: 0,
                message: "fetch not available in this context".into(),
            })?
            .call1(&global, &req)
            .map_err(js_err)?;

        let resp: Response = JsFuture::from(promise.dyn_into::<js_sys::Promise>().map_err(js_err)?)
            .await
            .map_err(js_err)?
            .dyn_into()
            .map_err(js_err)?;

        let status = resp.status();

        let resp_headers = resp.headers();
        let headers = extract_headers(&resp_headers);

        let body_buf = JsFuture::from(resp.array_buffer().map_err(js_err)?)
            .await
            .map_err(js_err)?;
        let body = js_sys::Uint8Array::new(&body_buf).to_vec();

        log::trace!(
            "[WasmTransport] {} {} → {}",
            method_str,
            &request.url,
            status
        );

        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

fn extract_headers(headers: &Headers) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let iterator = match js_sys::try_iter(headers) {
        Ok(Some(iter)) => iter,
        Ok(None) => {
            warn!("response headers not iterable");
            return result;
        }
        Err(e) => {
            warn!("failed to iterate response headers: {:?}", e);
            return result;
        }
    };
    for entry in iterator {
        match entry {
            Ok(pair) => {
                let array = js_sys::Array::from(&pair);
                if let (Some(key), Some(value)) =
                    (array.get(0).as_string(), array.get(1).as_string())
                {
                    result.push((key, value));
                }
            }
            Err(e) => {
                warn!("skipping header entry: {:?}", e);
            }
        }
    }
    result
}

fn js_err(e: wasm_bindgen::JsValue) -> Error {
    let message = e
        .as_string()
        .or_else(|| {
            js_sys::Reflect::get(&e, &"message".into())
                .ok()?
                .as_string()
        })
        .unwrap_or_else(|| format!("{e:?}"));
    Error::Xrpc { status: 0, message }
}
