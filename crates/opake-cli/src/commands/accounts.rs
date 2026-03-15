use anyhow::Result;
use clap::Args;

use crate::config::FileStorage;

#[derive(Args)]
/// List all logged-in accounts
pub struct AccountsCommand {}

impl AccountsCommand {
    pub fn run(self, storage: &FileStorage) -> Result<()> {
        let config = storage.load_config_anyhow()?;

        if config.accounts.is_empty() {
            println!("no accounts — run `opake login` to add one");
            return Ok(());
        }

        for (did, account) in &config.accounts {
            let marker = if config.default_did.as_deref() == Some(did) {
                "*"
            } else {
                " "
            };
            println!(
                "{} {} ({}) — {}",
                marker, account.handle, did, account.pds_url
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AccountEntry, Config};
    use crate::utils::test_harness::test_storage;
    use std::collections::BTreeMap;

    #[test]
    fn run_with_no_config_errors() {
        let (_dir, storage) = test_storage();
        let cmd = AccountsCommand {};
        assert!(cmd.run(&storage).is_err());
    }

    #[test]
    fn run_with_empty_accounts_succeeds() {
        let (_dir, storage) = test_storage();
        storage
            .save_config_anyhow(&Config {
                default_did: None,
                accounts: BTreeMap::new(),
            })
            .unwrap();

        let cmd = AccountsCommand {};
        cmd.run(&storage).unwrap();
    }

    #[test]
    fn run_with_accounts_succeeds() {
        let (_dir, storage) = test_storage();
        let mut accounts = BTreeMap::new();
        accounts.insert(
            "did:plc:alice".into(),
            AccountEntry {
                pds_url: "https://pds.alice".into(),
                handle: "alice.test".into(),
            },
        );
        storage
            .save_config_anyhow(&Config {
                default_did: Some("did:plc:alice".into()),
                accounts,
            })
            .unwrap();

        let cmd = AccountsCommand {};
        cmd.run(&storage).unwrap();
    }
}
