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
    Bytes {
        data: Vec<u8>,
        content_type: String,
    },
    /// URL-encoded form body (`application/x-www-form-urlencoded`).
    /// Used for OAuth token requests.
    Form(Vec<(String, String)>),
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
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Case-insensitive header lookup. Returns the first matching value.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

impl RequestBody {
    /// URL-encode form parameters into a `key=value&key=value` string.
    pub fn encode_form(params: &[(String, String)]) -> String {
        params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }
}

/// The only thing a platform needs to provide: send an HTTP request, get bytes back.
/// CLI implements this with reqwest, the SPA with browser fetch via web_sys.
pub trait Transport {
    fn send(
        &self,
        request: HttpRequest,
    ) -> impl std::future::Future<Output = Result<HttpResponse, Error>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_form_basic() {
        let params = vec![
            ("grant_type".into(), "authorization_code".into()),
            ("code".into(), "abc123".into()),
        ];
        assert_eq!(
            RequestBody::encode_form(&params),
            "grant_type=authorization_code&code=abc123"
        );
    }

    #[test]
    fn encode_form_special_characters() {
        let params = vec![
            (
                "redirect_uri".into(),
                "https://example.com/callback?foo=bar".into(),
            ),
            ("scope".into(), "atproto transition:generic".into()),
        ];
        let encoded = RequestBody::encode_form(&params);
        assert!(encoded.contains("redirect_uri=https%3A%2F%2Fexample.com%2Fcallback%3Ffoo%3Dbar"));
        assert!(encoded.contains("scope=atproto%20transition%3Ageneric"));
    }

    #[test]
    fn encode_form_empty() {
        assert_eq!(RequestBody::encode_form(&[]), "");
    }
}
