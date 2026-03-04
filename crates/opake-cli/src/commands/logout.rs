use anyhow::Result;
use clap::Args;

use crate::config::{resolve_handle_or_did, FileStorage};

#[derive(Args)]
/// Remove an account
pub struct LogoutCommand {
    /// Handle or DID of the account to remove
    account: String,
}

impl LogoutCommand {
    pub fn run(self, storage: &FileStorage) -> Result<()> {
        let cfg = storage.load_config_anyhow()?;
        let did = resolve_handle_or_did(&cfg, &self.account)?;
        let handle = cfg
            .accounts
            .get(&did)
            .map(|a| a.handle.as_str())
            .unwrap_or("unknown");

        println!("logging out {} ({})", handle, did);

        storage.remove_account(&did)?;

        println!("done");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AccountConfig, Config};
    use crate::utils::test_harness::test_storage;
    use std::collections::BTreeMap;

    #[test]
    fn logout_removes_account() {
        let (_dir, storage) = test_storage();
        let mut accounts = BTreeMap::new();
        accounts.insert(
            "did:plc:alice".into(),
            AccountConfig {
                pds_url: "https://pds.alice".into(),
                handle: "alice.test".into(),
            },
        );
        storage
            .save_config_anyhow(&Config {
                default_did: Some("did:plc:alice".into()),
                accounts,
                appview_url: None,
            })
            .unwrap();

        let cmd = LogoutCommand {
            account: "alice.test".into(),
        };
        cmd.run(&storage).unwrap();

        let loaded = storage.load_config_anyhow().unwrap();
        assert!(loaded.accounts.is_empty());
        assert!(loaded.default_did.is_none());
    }

    #[test]
    fn logout_unknown_handle_errors() {
        let (_dir, storage) = test_storage();
        let mut accounts = BTreeMap::new();
        accounts.insert(
            "did:plc:alice".into(),
            AccountConfig {
                pds_url: "https://pds.alice".into(),
                handle: "alice.test".into(),
            },
        );
        storage
            .save_config_anyhow(&Config {
                default_did: Some("did:plc:alice".into()),
                accounts,
                appview_url: None,
            })
            .unwrap();

        let cmd = LogoutCommand {
            account: "nobody.test".into(),
        };
        assert!(cmd.run(&storage).is_err());
    }
}
