use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Args, Subcommand};
use log::debug;
use opake_core::client::Session;
use opake_core::crypto::OsRng;
use opake_core::pairing;
use opake_core::records::PairRequest;

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

// The new-device flow runs without an identity, so it goes through the
// storage-backed pairing free functions rather than `Opake::for_account`.
// The ephemeral private key stays in `ctx.storage` for the duration.
async fn request(ctx: &CommandContext, args: RequestArgs) -> Result<Option<Session>> {
    let mut client = session::load_client(&ctx.storage, &ctx.did)?;

    if identity::load_identity(&ctx.storage, &ctx.did).is_ok() {
        anyhow::bail!(
            "this device already has an encryption identity for {}. \
             If you need to replace it, delete the identity file first.",
            ctx.did
        );
    }

    let info = pairing::create_pair_request(
        &mut client,
        &ctx.storage,
        &ctx.did,
        &Utc::now().to_rfc3339(),
        &mut OsRng,
    )
    .await?;

    println!("Pairing request created.");
    // Fingerprint the X25519 half — short and stable, matches what the
    // approving device displays. The ML-KEM half is 1184 bytes; printing
    // its fingerprint adds nothing for human comparison.
    println!("Fingerprint: {}", fingerprint(&info.x25519_ephemeral_public_key));
    println!();
    println!("Run `opake pair approve` on your existing device.");
    println!("Waiting for response...");

    let interval = std::time::Duration::from_secs(args.interval);
    loop {
        if pairing::try_complete_pair(&mut client, &ctx.storage, &ctx.did, &info.rkey).await? {
            break;
        }
        debug!("no matching response yet, sleeping {}s", args.interval);
        tokio::time::sleep(interval).await;
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
            .decode(&request.x25519_ephemeral_key.encoded)
            .context("invalid base64 in pair request X25519 ephemeral key")?;

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

    let x25519_bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.x25519_ephemeral_key.encoded)
        .context("invalid base64 in X25519 ephemeral key")?;
    let x25519_pubkey: [u8; 32] = x25519_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("X25519 ephemeral key must be 32 bytes"))?;

    let ml_kem_bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.ml_kem_ephemeral_key.encoded)
        .context("invalid base64 in ML-KEM-768 ephemeral key")?;
    let ml_kem_pubkey: [u8; 1184] = ml_kem_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("ML-KEM-768 ephemeral key must be 1184 bytes"))?;

    opake
        .approve_pair_request(request_uri, &x25519_pubkey, &ml_kem_pubkey)
        .await?;

    println!("Identity sent. The other device should receive it shortly.");
    Ok(None)
}

use base64::Engine;
