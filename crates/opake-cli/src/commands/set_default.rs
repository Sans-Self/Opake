use anyhow::Result;
use clap::Args;

use crate::config;

#[derive(Args)]
/// Set the default account
pub struct SetDefaultCommand {
    /// Handle or DID of the account to make default
    account: String,
}

impl SetDefaultCommand {
    pub fn run(self) -> Result<()> {
        let mut cfg = config::load_config()?;
        let did = config::resolve_handle_or_did(&cfg, &self.account)?;

        anyhow::ensure!(cfg.accounts.contains_key(&did), "no account for {did}");

        cfg.default_did = Some(did.clone());
        config::save_config(&cfg)?;

        let handle = &cfg.accounts[&did].handle;
        println!("default account set to {} ({})", handle, did);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AccountConfig, Config};
    use crate::utils::test_harness::with_test_dir;
    use std::collections::BTreeMap;

    fn two_account_config() -> Config {
        let mut accounts = BTreeMap::new();
        accounts.insert(
            "did:plc:alice".into(),
            AccountConfig {
                pds_url: "https://pds.alice".into(),
                handle: "alice.test".into(),
            },
        );
        accounts.insert(
            "did:plc:bob".into(),
            AccountConfig {
                pds_url: "https://pds.bob".into(),
                handle: "bob.test".into(),
            },
        );
        Config {
            default_did: Some("did:plc:alice".into()),
            accounts,
            appview_url: None,
        }
    }

    #[test]
    fn set_default_by_handle() {
        with_test_dir(|_| {
            config::save_config(&two_account_config()).unwrap();

            let cmd = SetDefaultCommand {
                account: "bob.test".into(),
            };
            cmd.run().unwrap();

            let loaded = config::load_config().unwrap();
            assert_eq!(loaded.default_did.as_deref(), Some("did:plc:bob"));
        });
    }

    #[test]
    fn set_default_by_did() {
        with_test_dir(|_| {
            config::save_config(&two_account_config()).unwrap();

            let cmd = SetDefaultCommand {
                account: "did:plc:bob".into(),
            };
            cmd.run().unwrap();

            let loaded = config::load_config().unwrap();
            assert_eq!(loaded.default_did.as_deref(), Some("did:plc:bob"));
        });
    }

    #[test]
    fn set_default_unknown_handle_errors() {
        with_test_dir(|_| {
            config::save_config(&two_account_config()).unwrap();

            let cmd = SetDefaultCommand {
                account: "nobody.test".into(),
            };
            let err = cmd.run().unwrap_err();
            assert!(err.to_string().contains("nobody.test"));
        });
    }

    #[test]
    fn set_default_unknown_did_errors() {
        with_test_dir(|_| {
            config::save_config(&two_account_config()).unwrap();

            let cmd = SetDefaultCommand {
                account: "did:plc:unknown".into(),
            };
            let err = cmd.run().unwrap_err();
            assert!(err.to_string().contains("did:plc:unknown"));
        });
    }
}
