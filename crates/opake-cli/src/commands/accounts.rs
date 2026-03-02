use anyhow::Result;
use clap::Args;

use crate::config;

#[derive(Args)]
/// List all logged-in accounts
pub struct AccountsCommand {}

impl AccountsCommand {
    pub fn run(self) -> Result<()> {
        let config = config::load_config()?;

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
    use crate::config::{AccountConfig, Config};
    use crate::utils::test_harness::with_test_dir;
    use std::collections::BTreeMap;

    #[test]
    fn run_with_no_config_errors() {
        with_test_dir(|_| {
            let cmd = AccountsCommand {};
            assert!(cmd.run().is_err());
        });
    }

    #[test]
    fn run_with_empty_accounts_succeeds() {
        with_test_dir(|_| {
            config::save_config(&Config {
                default_did: None,
                accounts: BTreeMap::new(),
                appview_url: None,
            })
            .unwrap();

            let cmd = AccountsCommand {};
            cmd.run().unwrap();
        });
    }

    #[test]
    fn run_with_accounts_succeeds() {
        with_test_dir(|_| {
            let mut accounts = BTreeMap::new();
            accounts.insert(
                "did:plc:alice".into(),
                AccountConfig {
                    pds_url: "https://pds.alice".into(),
                    handle: "alice.test".into(),
                },
            );
            config::save_config(&Config {
                default_did: Some("did:plc:alice".into()),
                accounts,
                appview_url: None,
            })
            .unwrap();

            let cmd = AccountsCommand {};
            cmd.run().unwrap();
        });
    }
}
