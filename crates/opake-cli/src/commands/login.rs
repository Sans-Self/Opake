use anyhow::Result;
use chrono::Utc;
use clap::Args;
use log::debug;
use opake_core::client::{Session, XrpcClient};
use opake_core::resolve::resolve_pds_for_login;

use crate::config::{AccountConfig, FileStorage};
use crate::identity;
use crate::transport::ReqwestTransport;
use crate::utils::prefixed_get_env;

/// Resolve password from env var or a fallback function (e.g. stdin prompt).
pub fn resolve_password(
    env_value: Option<String>,
    prompt_fn: impl FnOnce() -> Result<String>,
) -> Result<String> {
    let password = match env_value {
        Some(p) => p,
        None => prompt_fn()?,
    };
    anyhow::ensure!(!password.is_empty(), "password cannot be empty");
    Ok(password)
}

fn prompt_password(identifier: &str, pds: &str) -> Result<String> {
    print!("Password for user {} on {}: ", identifier, pds);
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

#[derive(Args)]
/// Authenticate with your PDS
pub struct LoginCommand {
    /// Handle or DID (e.g. alice.bsky.social, did:plc:...)
    identifier: String,

    /// PDS URL override (e.g. https://pds.example.com). Resolved automatically if omitted.
    #[arg(long)]
    pds: Option<String>,

    /// Force legacy password-based authentication
    #[arg(long)]
    legacy: bool,

    /// Don't redirect to frontend after OAuth - show inline response instead
    #[arg(long)]
    no_redirect: bool,
}

impl LoginCommand {
    pub async fn execute(self, storage: &FileStorage) -> Result<Option<Session>> {
        debug!("Starting login command");

        // Resolve PDS, DID, and handle — either from --pds or by querying the network.
        let (pds_url, identifier, resolved_handle) = match self.pds {
            Some(pds) => (pds, self.identifier, None),
            None => {
                println!("Resolving PDS for {}...", self.identifier);
                let transport = ReqwestTransport::new();
                let (did, pds, handle) = resolve_pds_for_login(&transport, &self.identifier)
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!("failed to resolve PDS for '{}': {e}", self.identifier)
                    })?;
                debug!("resolved: did={did}, pds={pds}, handle={handle:?}");
                println!("Found PDS: {pds}");
                (pds, did, handle)
            }
        };

        if self.legacy {
            return Self::legacy_login(&pds_url, &identifier, storage).await;
        }

        match crate::oauth::try_oauth_login(
            &pds_url,
            &identifier,
            resolved_handle.as_deref(),
            storage,
            self.no_redirect,
        )
        .await
        {
            Ok(session) => Ok(Some(session)),
            Err(e) => {
                log::warn!("OAuth login failed, falling back to password authentication. Password auth is deprecated by AT Protocol and will stop working. Error: {e}");
                Self::legacy_login(&pds_url, &identifier, storage).await
            }
        }
    }

    async fn legacy_login(
        pds_url: &str,
        identifier: &str,
        storage: &FileStorage,
    ) -> Result<Option<Session>> {
        let password = resolve_password(prefixed_get_env("PASSWORD"), || {
            prompt_password(identifier, pds_url)
        })?;

        let transport = ReqwestTransport::new();
        let mut client = XrpcClient::new(transport, pds_url.to_string());

        let session = client.login(identifier.trim(), &password).await?.clone();

        let mut cfg = storage.load_config_anyhow().unwrap_or_default();

        cfg.add_account(
            session.did().to_owned(),
            AccountConfig {
                pds_url: pds_url.to_string(),
                handle: session.handle().to_owned(),
            },
        );

        storage.save_config_anyhow(&cfg)?;

        let has_local_identity = identity::load_identity(storage, session.did()).is_ok();
        let has_published_key = client
            .get_record(
                session.did(),
                opake_core::records::PUBLIC_KEY_COLLECTION,
                opake_core::records::PUBLIC_KEY_RKEY,
            )
            .await
            .is_ok();

        if !has_local_identity && has_published_key {
            // Existing identity on another device — don't generate a new one.
            println!("Logged in as {}", session.handle());
            println!();
            println!("This account has an existing encryption identity.");
            println!("Run `opake pair request` to transfer it from another device.");
            return Ok(Some(session));
        }

        let (identity, generated) =
            identity::ensure_identity(storage, session.did(), &mut opake_core::crypto::OsRng)?;

        if generated {
            println!("Generated new encryption keypair");
        }

        let public_key_bytes = identity.public_key_bytes()?;
        let verify_key_bytes = identity.verify_key_bytes()?;
        opake_core::resolve::publish_public_key(
            &mut client,
            &public_key_bytes,
            verify_key_bytes.as_ref(),
            &Utc::now().to_rfc3339(),
        )
        .await?;
        println!("Published encryption public key");

        println!("Logged in as {}", session.handle());

        Ok(Some(session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opake_core::client::{HttpRequest, HttpResponse, Transport};
    use opake_core::error::Error;

    /// Mock transport that returns a preconfigured response.
    struct MockTransport {
        response: HttpResponse,
    }

    impl MockTransport {
        fn success_session() -> Self {
            let body = serde_json::json!({
                "did": "did:plc:test123",
                "handle": "alice.test",
                "accessJwt": "eyJ.access.token",
                "refreshJwt": "eyJ.refresh.token",
            });
            Self {
                response: HttpResponse {
                    status: 200,
                    headers: vec![],
                    body: serde_json::to_vec(&body).unwrap(),
                },
            }
        }

        fn auth_failure() -> Self {
            let body = serde_json::json!({
                "error": "AuthenticationRequired",
                "message": "Invalid identifier or password",
            });
            Self {
                response: HttpResponse {
                    status: 401,
                    headers: vec![],
                    body: serde_json::to_vec(&body).unwrap(),
                },
            }
        }
    }

    impl Transport for MockTransport {
        async fn send(&self, _request: HttpRequest) -> Result<HttpResponse, Error> {
            Ok(self.response.clone())
        }
    }

    #[tokio::test]
    async fn test_login_successful_session() {
        let transport = MockTransport::success_session();
        let mut client = XrpcClient::new(transport, "https://pds.test".into());

        let session = client.login("alice.test", "s3cret").await.unwrap();

        assert_eq!(session.did(), "did:plc:test123");
        assert_eq!(session.handle(), "alice.test");
        match session {
            Session::Legacy(s) => {
                assert!(!s.access_jwt.is_empty());
                assert!(!s.refresh_jwt.is_empty());
            }
            _ => panic!("expected Legacy session"),
        }
    }

    #[tokio::test]
    async fn test_login_bad_credentials_returns_error() {
        let transport = MockTransport::auth_failure();
        let mut client = XrpcClient::new(transport, "https://pds.test".into());

        let result = client.login("alice.test", "wrong").await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("401"), "expected 401 in error: {err}");
    }

    #[test]
    fn test_resolve_password_from_env() {
        let result = resolve_password(Some("s3cret".into()), || {
            panic!("prompt should not be called when env is set")
        });
        assert_eq!(result.unwrap(), "s3cret");
    }

    #[test]
    fn test_resolve_password_falls_back_to_prompt() {
        let result = resolve_password(None, || Ok("from_prompt".into()));
        assert_eq!(result.unwrap(), "from_prompt");
    }

    #[test]
    fn test_resolve_password_propagates_prompt_error() {
        let result = resolve_password(None, || anyhow::bail!("stdin broke"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "stdin broke");
    }

    #[test]
    fn test_resolve_password_env_preserves_whitespace() {
        let result = resolve_password(Some("  spaced  ".into()), || {
            panic!("prompt should not be called")
        });
        assert_eq!(result.unwrap(), "  spaced  ");
    }

    #[test]
    fn test_resolve_password_rejects_empty_from_env() {
        let result = resolve_password(Some("".into()), || panic!("prompt should not be called"));
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_password_rejects_empty_from_prompt() {
        let result = resolve_password(None, || Ok("".into()));
        assert!(result.is_err());
    }
}
