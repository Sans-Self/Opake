use chrono::Utc;
use log::info;
use opake_core::client::{ReqwestTransport, Session, XrpcClient};
use opake_core::crypto::OsRng;
use opake_core::opake::Opake;

use crate::config::{resolve_handle_or_did, FileStorage};

/// CLI's concrete Opake type — reqwest transport, OS randomness, filesystem storage.
pub type CliOpake = Opake<ReqwestTransport, OsRng, FileStorage>;

/// Resolved account context passed to every command.
#[derive(Debug)]
pub struct CommandContext {
    pub did: String,
    pub pds_url: String,
    pub storage: FileStorage,
}

pub fn chrono_now_micros() -> u64 {
    Utc::now().timestamp_micros() as u64
}

/// Build an `Opake` for the given account from a `FileStorage`.
///
/// The single construction point shared by `CommandContext::opake` (user-
/// facing commands) and the daemon's per-task builder. Reads identity and
/// session from Storage, wires up a reqwest transport, and applies the
/// `OPAKE_INDEXER_URL` runtime override so env tweaks don't require a
/// rebuild. Storage is cloned so the returned Opake owns its own handle
/// for `#[signoff]` auto-persist.
pub async fn build_opake(storage: &FileStorage, did: &str) -> anyhow::Result<CliOpake> {
    let mut opake = Opake::for_account(
        storage.clone(),
        Some(did),
        ReqwestTransport::new(),
        OsRng,
        chrono_now_micros,
    )
    .await?;

    // Platform sleep for the dependent-operation visibility-gap retry — the
    // same tokio timer the SSE reconnect loop uses. Without it a fresh-
    // workspace mutation would 403 on the genesis-indexing race instead of
    // waiting the window out.
    opake.set_sleep_fn(Box::new(|d| Box::pin(tokio::time::sleep(d))));
    opake.set_identity_sleep_fn(std::rc::Rc::new(|d| Box::pin(tokio::time::sleep(d))));

    if let Ok(url) = std::env::var("OPAKE_INDEXER_URL") {
        opake.set_indexer_url(url);
    }

    Ok(opake)
}

impl CommandContext {
    /// Build an Opake context for this account.
    ///
    /// Thin forwarder over [`build_opake`] for ergonomics at command
    /// callsites (`ctx.opake().await?`).
    pub async fn opake(&self) -> anyhow::Result<CliOpake> {
        build_opake(&self.storage, &self.did).await
    }
}

/// Resolve `--as` flag (or default account) to a CommandContext.
pub fn resolve_context(
    storage: &FileStorage,
    as_flag: Option<&str>,
) -> anyhow::Result<CommandContext> {
    let config = storage.load_config_anyhow()?;

    let did = match as_flag {
        Some(input) => resolve_handle_or_did(&config, input)?,
        None => config
            .default_did
            .ok_or_else(|| anyhow::anyhow!("no default account: log in first"))?,
    };

    let account = config
        .accounts
        .get(&did)
        .ok_or_else(|| anyhow::anyhow!("no account for {did}"))?;

    Ok(CommandContext {
        did,
        pds_url: account.pds_url.clone(),
        storage: storage.clone(),
    })
}

/// Restore a saved session and build an authenticated XRPC client for a specific account.
pub fn load_client(
    storage: &FileStorage,
    did: &str,
) -> anyhow::Result<XrpcClient<ReqwestTransport>> {
    let config = storage.load_config_anyhow()?;
    let account = config
        .accounts
        .get(did)
        .ok_or_else(|| anyhow::anyhow!("no account for {did}: log in first"))?;
    let session: Session = storage.load_account_json(did, "session.json")?;
    let transport = ReqwestTransport::new();
    Ok(XrpcClient::with_session(
        transport,
        account.pds_url.clone(),
        session,
    ))
}

/// Persist a refreshed session to disk for a specific account.
pub fn persist_session(storage: &FileStorage, did: &str, session: &Session) -> anyhow::Result<()> {
    storage.save_account_json(did, "session.json", session)?;
    info!("persisted refreshed session tokens for {}", did);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AccountEntry, Config};
    use crate::utils::test_harness::test_storage;
    use opake_core::client::LegacySession;
    use std::collections::BTreeMap;
    use std::fs;

    fn fake_session() -> Session {
        Session::Legacy(LegacySession {
            did: "did:plc:test123".into(),
            handle: "alice.test".into(),
            access_jwt: "eyJ.access.token".into(),
            refresh_jwt: "eyJ.refresh.token".into(),
        })
    }

    fn setup_account(storage: &FileStorage, did: &str, pds_url: &str, handle: &str) {
        let mut accounts = BTreeMap::new();
        accounts.insert(
            did.to_string(),
            AccountEntry {
                pds_url: pds_url.into(),
                handle: handle.into(),
            },
        );
        storage
            .save_config_anyhow(&Config {
                default_did: Some(did.to_string()),
                accounts,
                ..Default::default()
            })
            .unwrap();
    }

    #[test]
    fn persist_and_load_session_roundtrip() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test123";
        setup_account(&storage, did, "https://pds.test", "alice.test");
        let session = fake_session();
        persist_session(&storage, did, &session).unwrap();

        let loaded: Session = storage.load_account_json(did, "session.json").unwrap();
        assert_eq!(loaded.did(), session.did());
        assert_eq!(loaded.handle(), session.handle());
    }

    #[test]
    fn load_client_without_session_errors() {
        let (_dir, storage) = test_storage();
        assert!(load_client(&storage, "did:plc:nobody").is_err());
    }

    #[test]
    fn resolve_context_uses_default_did() {
        let (_dir, storage) = test_storage();
        setup_account(&storage, "did:plc:alice", "https://pds.alice", "alice.test");
        let ctx = resolve_context(&storage, None).unwrap();
        assert_eq!(ctx.did, "did:plc:alice");
        assert_eq!(ctx.pds_url, "https://pds.alice");
    }

    #[test]
    fn resolve_context_with_did_flag() {
        let (_dir, storage) = test_storage();
        let mut accounts = BTreeMap::new();
        accounts.insert(
            "did:plc:alice".into(),
            AccountEntry {
                pds_url: "https://pds.alice".into(),
                handle: "alice.test".into(),
            },
        );
        accounts.insert(
            "did:plc:bob".into(),
            AccountEntry {
                pds_url: "https://pds.bob".into(),
                handle: "bob.test".into(),
            },
        );
        storage
            .save_config_anyhow(&Config {
                default_did: Some("did:plc:alice".into()),
                accounts,
                ..Default::default()
            })
            .unwrap();

        let ctx = resolve_context(&storage, Some("did:plc:bob")).unwrap();
        assert_eq!(ctx.did, "did:plc:bob");
        assert_eq!(ctx.pds_url, "https://pds.bob");
    }

    #[test]
    fn resolve_context_with_handle_flag() {
        let (_dir, storage) = test_storage();
        let mut accounts = BTreeMap::new();
        accounts.insert(
            "did:plc:alice".into(),
            AccountEntry {
                pds_url: "https://pds.alice".into(),
                handle: "alice.test".into(),
            },
        );
        accounts.insert(
            "did:plc:bob".into(),
            AccountEntry {
                pds_url: "https://pds.bob".into(),
                handle: "bob.test".into(),
            },
        );
        storage
            .save_config_anyhow(&Config {
                default_did: Some("did:plc:alice".into()),
                accounts,
                ..Default::default()
            })
            .unwrap();

        let ctx = resolve_context(&storage, Some("bob.test")).unwrap();
        assert_eq!(ctx.did, "did:plc:bob");
    }

    #[test]
    fn resolve_context_unknown_handle_errors() {
        let (_dir, storage) = test_storage();
        setup_account(&storage, "did:plc:alice", "https://pds.alice", "alice.test");
        let err = resolve_context(&storage, Some("nobody.test")).unwrap_err();
        assert!(err.to_string().contains("nobody.test"));
    }

    #[test]
    fn resolve_context_no_default_errors() {
        let (_dir, storage) = test_storage();
        storage
            .save_config_anyhow(&Config {
                default_did: None,
                accounts: BTreeMap::new(),
                ..Default::default()
            })
            .unwrap();
        let err = resolve_context(&storage, None).unwrap_err();
        assert!(err.to_string().contains("log in first"));
    }

    #[test]
    fn load_session_rejects_garbage_json() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did, "https://pds.test", "test.handle");
        storage.ensure_account_dir(did).unwrap();
        fs::write(
            storage.account_dir(did).join("session.json"),
            "not json {{{",
        )
        .unwrap();
        assert!(load_client(&storage, did).is_err());
    }

    #[test]
    fn load_session_rejects_valid_json_wrong_schema() {
        let (_dir, storage) = test_storage();
        let did = "did:plc:test";
        setup_account(&storage, did, "https://pds.test", "test.handle");
        storage.ensure_account_dir(did).unwrap();
        fs::write(
            storage.account_dir(did).join("session.json"),
            r#"{"name": "bob", "age": 42}"#,
        )
        .unwrap();
        assert!(load_client(&storage, did).is_err());
    }
}
