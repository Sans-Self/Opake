use anyhow::Result;
use clap::Args;

use crate::config::{resolve_handle_or_did, FileStorage};

#[derive(Args)]
/// Set the default account
pub struct SetDefaultCommand {
    /// Handle or DID of the account to make default
    account: String,
}

impl SetDefaultCommand {
    pub fn run(self, storage: &FileStorage) -> Result<()> {
        let mut cfg = storage.load_config_anyhow()?;
        let did = resolve_handle_or_did(&cfg, &self.account)?;

        cfg.set_default(&did)?;
        storage.save_config_anyhow(&cfg)?;

        let handle = &cfg.accounts[&did].handle;
        println!("default account set to {} ({})", handle, did);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AccountEntry, Config};
    use crate::utils::test_harness::test_storage;
    use std::collections::BTreeMap;

    fn two_account_config() -> Config {
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
        Config {
            default_did: Some("did:plc:alice".into()),
            accounts,
        }
    }

    #[test]
    fn set_default_by_handle() {
        let (_dir, storage) = test_storage();
        storage.save_config_anyhow(&two_account_config()).unwrap();

        let cmd = SetDefaultCommand {
            account: "bob.test".into(),
        };
        cmd.run(&storage).unwrap();

        let loaded = storage.load_config_anyhow().unwrap();
        assert_eq!(loaded.default_did.as_deref(), Some("did:plc:bob"));
    }

    #[test]
    fn set_default_by_did() {
        let (_dir, storage) = test_storage();
        storage.save_config_anyhow(&two_account_config()).unwrap();

        let cmd = SetDefaultCommand {
            account: "did:plc:bob".into(),
        };
        cmd.run(&storage).unwrap();

        let loaded = storage.load_config_anyhow().unwrap();
        assert_eq!(loaded.default_did.as_deref(), Some("did:plc:bob"));
    }

    #[test]
    fn set_default_unknown_handle_errors() {
        let (_dir, storage) = test_storage();
        storage.save_config_anyhow(&two_account_config()).unwrap();

        let cmd = SetDefaultCommand {
            account: "nobody.test".into(),
        };
        let err = cmd.run(&storage).unwrap_err();
        assert!(err.to_string().contains("nobody.test"));
    }

    #[test]
    fn set_default_unknown_did_errors() {
        let (_dir, storage) = test_storage();
        storage.save_config_anyhow(&two_account_config()).unwrap();

        let cmd = SetDefaultCommand {
            account: "did:plc:unknown".into(),
        };
        let err = cmd.run(&storage).unwrap_err();
        assert!(err.to_string().contains("did:plc:unknown"));
    }
}
