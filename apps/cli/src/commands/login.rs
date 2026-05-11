use crate::config::{AccountEntry, FileStorage};
use crate::identity;
use crate::utils::prefixed_get_env;
use anyhow::Result;
use chrono::Utc;
use clap::Args;
use log::debug;
use opake_core::client::ReqwestTransport;
use opake_core::client::Transport;
use opake_core::client::{Session, XrpcClient};
use opake_core::crypto::{format_mnemonic_grid, generate_mnemonic, OsRng};
use opake_core::storage::Identity;

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
    crate::prompt::input(&format!("Password for {identifier} on {pds}: "))
}

/// Authenticate with your PDS
///
/// Uses OAuth by default. Falls back to legacy password authentication
/// if the PDS does not support OAuth.
#[derive(Args)]
#[command(after_help = "\
Examples:
  opake account login alice.bsky.social
  opake account login alice.bsky.social --legacy
  opake account login did:plc:abc123 --pds https://pds.example.com")]
pub struct LoginCommand {
    /// Handle or DID (e.g. alice.bsky.social, did:plc:...)
    identifier: String,

    /// PDS URL override (e.g. https://pds.example.com). Resolved automatically if omitted.
    #[arg(long, value_name = "URL")]
    pds: Option<String>,

    /// Force legacy password-based authentication
    #[arg(long)]
    legacy: bool,

    /// Don't redirect to frontend after OAuth - show inline response instead
    #[arg(long)]
    no_redirect: bool,

    /// Overwrite existing encryption identity (generates new keypair even if one exists on PDS)
    #[arg(long)]
    force: bool,
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
                let (did, pds, handle) = opake_core::resolve::resolve_pds_for_login_with_dns(
                    &transport,
                    &self.identifier,
                )
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
            return Self::legacy_login(&pds_url, &identifier, storage, self.force).await;
        }

        match crate::oauth::try_oauth_login(
            &pds_url,
            &identifier,
            resolved_handle.as_deref(),
            storage,
            self.no_redirect,
            self.force,
        )
        .await
        {
            Ok(session) => Ok(Some(session)),
            Err(e) => {
                log::warn!("OAuth login failed, falling back to password authentication. Password auth is deprecated by AT Protocol and will stop working. Error: {e}");
                Self::legacy_login(&pds_url, &identifier, storage, self.force).await
            }
        }
    }

    async fn legacy_login(
        pds_url: &str,
        identifier: &str,
        storage: &FileStorage,
        force: bool,
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
            AccountEntry {
                pds_url: pds_url.to_string(),
                handle: session.handle().to_owned(),
            },
        );

        storage.save_config_anyhow(&cfg)?;

        ensure_identity_and_publish(&mut client, storage, session.did(), force).await?;
        println!("Logged in as {}", session.handle());

        // Return the client's current session, not the original. If
        // ensure_identity_and_publish triggered a refresh, the original
        // session is stale and persisting it would lock the user out.
        let final_session = client
            .session()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("XRPC client lost its session during login"))?;

        Ok(Some(final_session))
    }
}

/// Check for existing published key, prompt for --force confirmation if needed,
/// generate identity from seed phrase (or load existing), and publish the
/// public key. Shared by both legacy and OAuth login paths.
pub async fn ensure_identity_and_publish(
    client: &mut XrpcClient<impl Transport>,
    storage: &FileStorage,
    did: &str,
    force: bool,
) -> Result<()> {
    // Single load attempt — avoids reading identity.json twice.
    let existing = identity::load_and_migrate(storage, did, &mut OsRng)?;
    let has_published_key = client
        .get_record(
            did,
            opake_core::records::PUBLIC_KEY_COLLECTION,
            opake_core::records::PUBLIC_KEY_RKEY,
        )
        .await
        .is_ok();

    // Published key exists, no local identity, no --force → can't proceed.
    if existing.is_none() && has_published_key && !force {
        println!();
        println!("This account has an existing encryption identity.");
        println!("Run `opake pair request` to transfer it from another device,");
        println!("`opake recover` to restore from a seed phrase,");
        println!(
            "or `opake account login --force` to generate a new identity (invalidates old one)."
        );
        return Ok(());
    }

    // Published key exists + --force → scary confirmation before overwriting.
    if existing.is_none() && has_published_key && force {
        println!();
        crate::prompt::confirm_exact(
            "WARNING: --force will generate a new encryption identity.\n\
             All data encrypted to the old identity will become permanently unreadable.\n",
            "This will brick my data and I am okay with that",
        )?;
        println!("Proceeding with new identity generation.");
    }

    // Use existing identity, or generate new from seed phrase.
    let identity = match existing {
        Some(identity) => identity,
        None => {
            let fresh = generate_identity_from_seed_phrase(did)?;
            identity::save_identity(storage, did, &fresh)?;
            fresh
        }
    };

    let public_key_bytes = identity.x25519_public_key_bytes()?;
    let ml_kem_public_key_bytes = identity.ml_kem_public_key_bytes()?;
    let verify_key_bytes = identity.verify_key_bytes()?;
    opake_core::resolve::publish_public_key(
        client,
        &public_key_bytes,
        &ml_kem_public_key_bytes,
        verify_key_bytes.as_ref(),
        &Utc::now().to_rfc3339(),
    )
    .await?;
    println!("Published encryption public key");

    Ok(())
}

/// Interactive seed phrase generation flow: generate mnemonic, display grid,
/// confirm 3 random words, derive identity.
fn generate_identity_from_seed_phrase(did: &str) -> Result<opake_core::storage::Identity> {
    let mnemonic = generate_mnemonic(&mut OsRng);

    println!();
    println!("Your seed phrase (write this down — it will NOT be shown again):");
    println!();
    print!("{}", format_mnemonic_grid(&mnemonic));
    println!();

    // Pick 3 random word positions for confirmation.
    let mut confirm_indices = pick_confirmation_indices(&mut OsRng);
    confirm_indices.sort();

    let words = mnemonic.words();
    for &idx in &confirm_indices {
        let expected = &words[idx];
        let entered = crate::prompt::input(&format!("Enter word #{}: ", idx + 1))?;
        if entered != expected.as_str() {
            anyhow::bail!(
                "word #{} incorrect (expected {expected:?}, got {entered:?}). \
                 Please try again with `opake account login`.",
                idx + 1,
            );
        }
    }

    println!("Seed phrase confirmed.");

    // Offer to save the seed phrase to a file.
    offer_save_seed_file(&mnemonic)?;

    let identity = Identity::from_mnemonic(&mnemonic, did);
    Ok(identity)
}

/// Prompt the user to save the seed phrase grid to a .txt file.
/// Loops on write failure so the user can fix the path.
fn offer_save_seed_file(mnemonic: &opake_core::crypto::Mnemonic) -> Result<()> {
    println!();
    println!("Save seed phrase to a file? Enter a path, or press Enter to skip.");

    let default_path = std::env::current_dir()
        .unwrap_or_default()
        .join("opake-seed-phrase.txt");

    loop {
        let trimmed = crate::prompt::input_with_default(">", &default_path.display().to_string())?;

        if trimmed == default_path.display().to_string() {
            // Use default path.
            let path = &default_path;
            match write_seed_file(path, mnemonic) {
                Ok(()) => {
                    println!("Saved to {}", path.display());
                    return Ok(());
                }
                Err(e) => {
                    println!("Failed to write {}: {e}", path.display());
                    println!("Enter a different path, or type 'skip' to continue without saving:");
                    continue;
                }
            }
        }

        if trimmed == "skip" {
            println!("Skipping file save.");
            return Ok(());
        }

        let path = std::path::PathBuf::from(trimmed);
        match write_seed_file(&path, mnemonic) {
            Ok(()) => {
                println!("Saved to {}", path.display());
                return Ok(());
            }
            Err(e) => {
                println!("Failed to write {}: {e}", path.display());
                println!("Try a different path, or type 'skip':");
            }
        }
    }
}

/// Write the seed phrase grid to a file with restrictive permissions.
fn write_seed_file(path: &std::path::Path, mnemonic: &opake_core::crypto::Mnemonic) -> Result<()> {
    let grid = format_mnemonic_grid(mnemonic);
    crate::config::FileStorage::write_sensitive_file(path, grid)?;
    Ok(())
}

/// Pick 3 distinct random indices from 0..23 for word confirmation.
fn pick_confirmation_indices(
    rng: &mut (impl opake_core::crypto::CryptoRng + opake_core::crypto::RngCore),
) -> [usize; 3] {
    let mut indices = [0usize; 3];
    let mut count = 0;
    while count < 3 {
        let idx = (rng.next_u32() as usize) % 24;
        if !indices[..count].contains(&idx) {
            indices[count] = idx;
            count += 1;
        }
    }
    indices
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
