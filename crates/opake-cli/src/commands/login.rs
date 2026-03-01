use anyhow::Result;
use clap::Args;
use log::debug;
use opake_core::client::{Session, XrpcClient};

use crate::commands::Execute;
use crate::config::{self, AccountConfig};
use crate::identity;
use crate::transport::ReqwestTransport;
use crate::utils::prefixed_get_env;

use std::collections::BTreeMap;

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
    /// PDS URL (e.g. https://pds.example.com)
    #[arg(long)]
    pds: String,

    /// Handle or DID
    #[arg(long)]
    identifier: String,
}

impl Execute for LoginCommand {
    async fn execute(self) -> Result<Option<Session>> {
        debug!("Starting login command");

        let password = resolve_password(prefixed_get_env("PASSWORD"), || {
            prompt_password(&self.identifier, &self.pds)
        })?;

        let transport = ReqwestTransport::new();
        let mut client = XrpcClient::new(transport, self.pds.clone());

        let session = client.login(self.identifier.trim(), &password).await?;

        // Register this account in the config. Merge into existing accounts
        // if present, set as default if it's the first one.
        let mut cfg = config::load_config().unwrap_or(config::Config {
            default_did: None,
            accounts: BTreeMap::new(),
        });

        cfg.accounts.insert(
            session.did.clone(),
            AccountConfig {
                pds_url: self.pds.clone(),
                handle: session.handle.clone(),
            },
        );

        if cfg.default_did.is_none() {
            cfg.default_did = Some(session.did.clone());
        }

        config::save_config(&cfg)?;

        let (_, generated) =
            identity::ensure_identity(&session.did, &mut opake_core::crypto::OsRng)?;

        if generated {
            println!("Generated new encryption keypair");
        }

        println!("Logged in as {}", session.handle);

        Ok(Some(session.clone()))
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

        assert_eq!(session.did, "did:plc:test123");
        assert_eq!(session.handle, "alice.test");
        assert!(!session.access_jwt.is_empty());
        assert!(!session.refresh_jwt.is_empty());
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
        // env vars aren't trimmed — spaces in passwords are valid
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
        // user just hits enter — trims to empty
        let result = resolve_password(None, || Ok("".into()));
        assert!(result.is_err());
    }
}
