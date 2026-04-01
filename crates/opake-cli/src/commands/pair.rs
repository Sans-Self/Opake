use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Args, Subcommand};
use log::debug;
use opake_core::atproto;
use opake_core::client::Session;
use opake_core::crypto::OsRng;
use opake_core::pairing;
use opake_core::records::{PairRequest, PairResponse, PAIR_RESPONSE_COLLECTION};

use crate::commands::Execute;
use crate::identity;
use crate::session::{self, CommandContext};

/// Transfer encryption identity between devices
///
/// Uses an ephemeral key exchange via PDS relay records. Compare
/// fingerprints on both devices to verify the pairing.
#[derive(Args)]
#[command(after_help = "\
Workflow:
  New device:       opake pair request
  Existing device:  opake pair approve")]
pub struct PairCommand {
    #[command(subcommand)]
    action: PairAction,
}

#[derive(Subcommand)]
enum PairAction {
    /// Request identity transfer from an existing device (run on the NEW device)
    Request(RequestArgs),
    /// Approve a pending pairing request (run on the EXISTING device)
    Approve(ApproveArgs),
}

#[derive(Args)]
struct RequestArgs {
    /// Polling interval in seconds
    #[arg(long, default_value = "3")]
    interval: u64,
}

#[derive(Args)]
struct ApproveArgs;

impl Execute for PairCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        match self.action {
            PairAction::Request(args) => request(ctx, args).await,
            PairAction::Approve(_args) => approve(ctx).await,
        }
    }
}

/// Format an ephemeral key fingerprint for visual SAS comparison.
fn fingerprint(key: &[u8; 32]) -> String {
    key.iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

// request runs on the NEW device — no identity exists yet, so ctx.opake()
// can't be used (it requires identity). Keep session::load_client for the
// authenticated PDS client and use raw pairing functions.
async fn request(ctx: &CommandContext, args: RequestArgs) -> Result<Option<Session>> {
    let mut client = session::load_client(&ctx.storage, &ctx.did)?;

    // Bail if this device already has an identity — use `opake account login` instead.
    if identity::load_identity(&ctx.storage, &ctx.did).is_ok() {
        anyhow::bail!(
            "this device already has an encryption identity for {}. \
             If you need to replace it, delete the identity file first.",
            ctx.did
        );
    }

    let (record_ref, ephemeral_keypair) =
        pairing::create_pair_request(&mut client, &Utc::now().to_rfc3339(), &mut OsRng).await?;

    let request_uri = &record_ref.uri;
    let request_at_uri = atproto::parse_at_uri(request_uri)?;

    println!("Pairing request created.");
    println!(
        "Fingerprint: {}",
        fingerprint(&ephemeral_keypair.public_key)
    );
    println!();
    println!("Run `opake pair approve` on your existing device.");
    println!("Waiting for response...");

    // Poll for a matching pairResponse record.
    let interval = std::time::Duration::from_secs(args.interval);
    let response: PairResponse = loop {
        tokio::time::sleep(interval).await;
        debug!("polling for pair response...");

        let page = client
            .list_records(PAIR_RESPONSE_COLLECTION, Some(100), None)
            .await?;

        let found = page.records.into_iter().find(|entry| {
            serde_json::from_value::<PairResponse>(entry.value.clone())
                .map(|r| r.request == *request_uri)
                .unwrap_or(false)
        });

        if let Some(entry) = found {
            break serde_json::from_value(entry.value)?;
        }
    };

    let received_identity = pairing::receive_pair_response(
        &mut client,
        &ctx.did,
        &response,
        &ephemeral_keypair.private_key,
    )
    .await?;

    identity::save_identity(&ctx.storage, &ctx.did, &received_identity)?;
    println!("Identity received and saved.");

    // Clean up both records.
    let response_page = client
        .list_records(PAIR_RESPONSE_COLLECTION, Some(100), None)
        .await?;
    let response_rkey = response_page
        .records
        .iter()
        .find(|entry| {
            serde_json::from_value::<PairResponse>(entry.value.clone())
                .map(|r| r.request == *request_uri)
                .unwrap_or(false)
        })
        .map(|entry| atproto::parse_at_uri(&entry.uri))
        .transpose()?
        .map(|uri| uri.rkey);

    if let Some(ref rkey) = response_rkey {
        pairing::cleanup_pair_records(&mut client, &request_at_uri.rkey, rkey).await?;
        debug!("cleaned up pairing records");
    }

    println!("Pairing complete.");
    Ok(None)
}

async fn approve(ctx: &CommandContext) -> Result<Option<Session>> {
    let mut opake = ctx.opake().await?;

    let entries = opake.list_pair_requests().await?;

    if entries.is_empty() {
        println!("No pending pairing requests.");
        return Ok(None);
    }

    println!("Pending pairing requests:\n");

    let mut requests: Vec<(String, PairRequest)> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let request: PairRequest = serde_json::from_value(entry.value.clone())
            .context("failed to parse pair request record")?;

        let ephemeral_key_bytes = base64::engine::general_purpose::STANDARD
            .decode(&request.ephemeral_key.encoded)
            .context("invalid base64 in pair request ephemeral key")?;

        let fp = if ephemeral_key_bytes.len() == 32 {
            let arr: [u8; 32] = ephemeral_key_bytes.try_into().unwrap();
            fingerprint(&arr)
        } else {
            "(invalid key length)".to_string()
        };

        println!("  [{}] {} — fingerprint: {}", i + 1, request.created_at, fp);
        requests.push((entry.uri.clone(), request));
    }

    println!();
    let selection =
        crate::prompt::input(&format!("Approve which request? [1-{}] ", requests.len()))?;
    let choice: usize = selection.parse().context("invalid selection")?;
    anyhow::ensure!(
        choice >= 1 && choice <= requests.len(),
        "selection out of range"
    );

    let (ref request_uri, ref request) = requests[choice - 1];

    let ephemeral_key_bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.ephemeral_key.encoded)
        .context("invalid base64 in ephemeral key")?;
    let ephemeral_pubkey: [u8; 32] = ephemeral_key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("ephemeral key must be 32 bytes"))?;

    opake
        .approve_pair_request(request_uri, &ephemeral_pubkey)
        .await?;

    println!("Identity sent. The other device should receive it shortly.");
    Ok(None)
}

use base64::Engine;
