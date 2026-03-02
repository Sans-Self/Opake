use log::info;
use opake_core::client::{Session, XrpcClient};

use crate::config;
use crate::transport::ReqwestTransport;

/// Resolved account context passed to every command.
#[derive(Debug)]
pub struct CommandContext {
    pub did: String,
    #[allow(dead_code)] // will be used when load_client takes context directly
    pub pds_url: String,
}

/// Resolve `--as` flag (or default account) to a CommandContext.
pub fn resolve_context(as_flag: Option<&str>) -> anyhow::Result<CommandContext> {
    let config = config::load_config()?;

    let did = match as_flag {
        Some(input) => config::resolve_handle_or_did(&config, input)?,
        None => config
            .default_did
            .ok_or_else(|| anyhow::anyhow!("no default account: run `opake login` first"))?,
    };

    let account = config
        .accounts
        .get(&did)
        .ok_or_else(|| anyhow::anyhow!("no account for {did}"))?;

    Ok(CommandContext {
        did,
        pds_url: account.pds_url.clone(),
    })
}

/// Restore a saved session and build an authenticated XRPC client for a specific account.
pub fn load_client(did: &str) -> anyhow::Result<XrpcClient<ReqwestTransport>> {
    let config = config::load_config()?;
    let account = config
        .accounts
        .get(did)
        .ok_or_else(|| anyhow::anyhow!("no account for {did}: run `opake login` first"))?;
    let session: Session = config::load_account_json(did, "session.json")?;
    let transport = ReqwestTransport::new();
    Ok(XrpcClient::with_session(
        transport,
        account.pds_url.clone(),
        session,
    ))
}

/// Extract the session if it was refreshed during this client's lifetime.
/// Commands return this to the dispatch layer for persistence.
pub fn refreshed_session(client: &XrpcClient<ReqwestTransport>) -> Option<Session> {
    if client.session_refreshed() {
        client.session().cloned()
    } else {
        None
    }
}

/// Persist a refreshed session to disk for a specific account.
pub fn persist_session(did: &str, session: &Session) -> anyhow::Result<()> {
    config::save_account_json(did, "session.json", session)?;
    info!("persisted refreshed session tokens for {}", did);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_harness::with_test_dir;
    use std::collections::BTreeMap;
    use std::fs;

    fn fake_session() -> Session {
        Session {
            did: "did:plc:test123".into(),
            handle: "alice.test".into(),
            access_jwt: "eyJ.access.token".into(),
            refresh_jwt: "eyJ.refresh.token".into(),
        }
    }

    fn setup_account(did: &str, pds_url: &str, handle: &str) {
        let mut accounts = BTreeMap::new();
        accounts.insert(
            did.to_string(),
            config::AccountConfig {
                pds_url: pds_url.into(),
                handle: handle.into(),
            },
        );
        config::save_config(&config::Config {
            default_did: Some(did.to_string()),
            accounts,
            appview_url: None,
        })
        .unwrap();
    }

    #[test]
    fn persist_and_load_session_roundtrip() {
        with_test_dir(|_| {
            let did = "did:plc:test123";
            setup_account(did, "https://pds.test", "alice.test");
            let session = fake_session();
            persist_session(did, &session).unwrap();

            let loaded: Session = config::load_account_json(did, "session.json").unwrap();
            assert_eq!(loaded.did, session.did);
            assert_eq!(loaded.handle, session.handle);
            assert_eq!(loaded.access_jwt, session.access_jwt);
            assert_eq!(loaded.refresh_jwt, session.refresh_jwt);
        });
    }

    #[test]
    fn load_client_without_session_errors() {
        with_test_dir(|_| {
            assert!(load_client("did:plc:nobody").is_err());
        });
    }

    #[test]
    fn resolve_context_uses_default_did() {
        with_test_dir(|_| {
            setup_account("did:plc:alice", "https://pds.alice", "alice.test");
            let ctx = resolve_context(None).unwrap();
            assert_eq!(ctx.did, "did:plc:alice");
            assert_eq!(ctx.pds_url, "https://pds.alice");
        });
    }

    #[test]
    fn resolve_context_with_did_flag() {
        with_test_dir(|_| {
            let mut accounts = BTreeMap::new();
            accounts.insert(
                "did:plc:alice".into(),
                config::AccountConfig {
                    pds_url: "https://pds.alice".into(),
                    handle: "alice.test".into(),
                },
            );
            accounts.insert(
                "did:plc:bob".into(),
                config::AccountConfig {
                    pds_url: "https://pds.bob".into(),
                    handle: "bob.test".into(),
                },
            );
            config::save_config(&config::Config {
                default_did: Some("did:plc:alice".into()),
                accounts,
                appview_url: None,
            })
            .unwrap();

            let ctx = resolve_context(Some("did:plc:bob")).unwrap();
            assert_eq!(ctx.did, "did:plc:bob");
            assert_eq!(ctx.pds_url, "https://pds.bob");
        });
    }

    #[test]
    fn resolve_context_with_handle_flag() {
        with_test_dir(|_| {
            let mut accounts = BTreeMap::new();
            accounts.insert(
                "did:plc:alice".into(),
                config::AccountConfig {
                    pds_url: "https://pds.alice".into(),
                    handle: "alice.test".into(),
                },
            );
            accounts.insert(
                "did:plc:bob".into(),
                config::AccountConfig {
                    pds_url: "https://pds.bob".into(),
                    handle: "bob.test".into(),
                },
            );
            config::save_config(&config::Config {
                default_did: Some("did:plc:alice".into()),
                accounts,
                appview_url: None,
            })
            .unwrap();

            let ctx = resolve_context(Some("bob.test")).unwrap();
            assert_eq!(ctx.did, "did:plc:bob");
        });
    }

    #[test]
    fn resolve_context_unknown_handle_errors() {
        with_test_dir(|_| {
            setup_account("did:plc:alice", "https://pds.alice", "alice.test");
            let err = resolve_context(Some("nobody.test")).unwrap_err();
            assert!(err.to_string().contains("nobody.test"));
        });
    }

    #[test]
    fn resolve_context_no_default_errors() {
        with_test_dir(|_| {
            config::save_config(&config::Config {
                default_did: None,
                accounts: BTreeMap::new(),
                appview_url: None,
            })
            .unwrap();
            let err = resolve_context(None).unwrap_err();
            assert!(err.to_string().contains("opake login"));
        });
    }

    #[test]
    fn load_session_rejects_garbage_json() {
        with_test_dir(|_| {
            let did = "did:plc:test";
            setup_account(did, "https://pds.test", "test.handle");
            config::ensure_account_dir(did).unwrap();
            fs::write(
                config::account_dir(did).join("session.json"),
                "not json {{{",
            )
            .unwrap();
            assert!(load_client(did).is_err());
        });
    }

    #[test]
    fn load_session_rejects_valid_json_wrong_schema() {
        with_test_dir(|_| {
            let did = "did:plc:test";
            setup_account(did, "https://pds.test", "test.handle");
            config::ensure_account_dir(did).unwrap();
            fs::write(
                config::account_dir(did).join("session.json"),
                r#"{"name": "bob", "age": 42}"#,
            )
            .unwrap();
            assert!(load_client(did).is_err());
        });
    }
}
