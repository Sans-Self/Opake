mod config;
mod transport;

use clap::{Parser, Subcommand};
use opake_core::client::XrpcClient;
use std::path::PathBuf;
use transport::ReqwestTransport;

#[derive(Parser)]
#[command(name = "opake", about = "Encrypted personal cloud on AT Protocol")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Authenticate with your PDS
    Login {
        /// PDS URL (e.g. https://pds.example.com)
        #[arg(long)]
        pds: String,

        /// Handle or DID
        #[arg(long)]
        identifier: String,

        /// App password
        #[arg(long)]
        password: String,
    },

    /// Upload and encrypt a file
    Upload {
        /// Path to the file to encrypt and upload
        path: PathBuf,

        /// Encrypt under a keyring instead of direct keys
        #[arg(long)]
        keyring: Option<String>,

        /// Comma-separated tags for categorization
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },

    /// Download and decrypt a file
    Download {
        /// AT URI of the document record
        uri: String,

        /// Output path (defaults to the original filename)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// List your documents
    Ls {
        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,
    },

    /// Delete a document
    Rm {
        /// AT URI of the document record
        uri: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Login { pds, identifier, password } => {
            let transport = ReqwestTransport::new();
            let mut client = XrpcClient::new(transport, pds);
            let session = client.login(&identifier, &password).await?;
            config::save_session(session)?;
            println!("logged in as {}", session.handle);
            Ok(())
        }
        Command::Upload { path, keyring, tags } => {
            let _client = config::load_client()?;
            let _ = (path, keyring, tags);
            todo!("read file, generate content key, encrypt, upload blob, create document record")
        }
        Command::Download { uri, output } => {
            let _client = config::load_client()?;
            let _ = (uri, output);
            todo!("fetch document record, resolve encryption, fetch blob, decrypt, write to disk")
        }
        Command::Ls { tag } => {
            let _client = config::load_client()?;
            let _ = tag;
            todo!("list document records via com.atproto.repo.listRecords")
        }
        Command::Rm { uri } => {
            let _client = config::load_client()?;
            let _ = uri;
            todo!("delete document record via com.atproto.repo.deleteRecord")
        }
    }
}
