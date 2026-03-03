# Opake

**/oʊˈpɑːk/** — like "opaque," but Dutch-flavored.

An encrypted personal cloud built on the [AT Protocol](https://atproto.com).

Opake uses your existing PDS as a storage and identity layer. Files are encrypted client-side with AES-256-GCM before upload — the PDS only ever sees ciphertext. Custom lexicons under `app.opake.cloud.*` give structure to documents, encryption metadata, and sharing grants.

Your data is opaque to everyone without the key. That's the point.

[Issue Tracker](https://issues.opake.app) · [Architecture](docs/ARCHITECTURE.md) · [Lexicons](lexicons/README.md)

## How It Works

```
plaintext file
  → encrypt with random AES-256-GCM key
  → upload ciphertext blob to PDS
  → wrap content key to owner's DID public key
  → store metadata as app.opake.cloud.document record
```

No modifications to the PDS. All crypto happens on your machine.

## Build From Source

Requires Rust 1.75+.

```sh
git clone <repo-url>
cd opake.dev
cargo build --release
```

Produces two binaries: `target/release/opake` (CLI) and `target/release/opake-appview` (indexer/API server).

## Usage

```sh
# authenticate with your PDS
opake login --pds https://pds.example.com --identifier alice.example.com

# log in to a second account
opake login --pds https://other-pds.example.com --identifier bob.other.com

# list accounts and switch default
opake accounts
opake set-default bob.other.com

# upload a file (encrypts + uploads)
opake upload photo.jpg --tags vacation,beach

# upload into a directory
opake upload photo.jpg --dir Photos

# organize files into directories
opake mkdir Photos
opake tree

# list your documents
opake ls
opake ls --long
opake ls --tag vacation

# use a specific account for any command
opake ls --as alice.example.com
opake upload doc.pdf --as did:plc:alice123

# download and decrypt (your own files)
opake download photo.jpg
opake download photo.jpg -o ~/Downloads/copy.jpg

# print a file to stdout (decrypt without saving)
opake cat notes.txt
opake cat Photos/notes.txt

# download a shared file from another user (via grant URI)
opake download --grant at://did:plc:abc/app.opake.cloud.grant/tid123

# delete (supports paths and recursive directory deletion)
opake rm photo.jpg
opake rm Photos/photo.jpg
opake rm -r Photos

# move and rename
opake mv photo.jpg Photos/
opake mv photo.jpg vacation-photo.jpg

# resolve a handle or DID to see their public key
opake resolve alice.example.com

# share a file with another user
opake share photo.jpg alice.example.com

# list grants you've shared
opake shared
opake shared --long

# revoke a share grant
opake revoke at://did:plc:abc/app.opake.cloud.grant/tid123

# remove an account
opake logout bob.other.com
```

Commands accept a filename, a path (`Photos/beach.jpg`), or an `at://` URI. If a filename matches multiple documents, you'll be prompted to use the full URI.

The `--as` flag works with document commands (`upload`, `download`, `ls`, `rm`, `mv`, `cat`, `tree`, `share`, `shared`, `revoke`) and accepts a handle or DID.

## AppView

The AppView is a separate binary (`opake-appview`) that indexes grants and keyrings from the AT Protocol firehose and serves them via a REST API. It enables grant discovery — "what's been shared with me?" — without scanning every PDS in the network.

See [docs/appview.md](docs/appview.md) for configuration, authentication, API endpoints, and deployment.

## Architecture

Four crates:

- **`opake-core`** — platform-agnostic library (compiles to WASM). Encryption, records, XRPC client, document operations.
- **`opake-cli`** — thin CLI wrapper. Config, session, identity persistence.
- **`opake-appview`** — Axum-based indexer and REST API. Jetstream firehose consumer, SQLite storage, DID-scoped Ed25519 auth.
- **`opake-derive`** — Proc-macro crate. `RedactedDebug` derive macro for secret-safe Debug output.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the encryption model, crate structure, and design decisions. See [docs/FLOWS.md](docs/FLOWS.md) for sequence diagrams of every operation.

## Roadmap

- [x] CLI foundation (auth, upload, download, ls, rm)
- [x] Client-side AES-256-GCM encryption
- [x] Asymmetric key wrapping (x25519-hkdf-a256kw)
- [x] Automatic token refresh
- [x] Multi-account support (--as flag, logout, set-default, accounts)
- [x] Public key auto-publish on login (app.opake.cloud.publicKey record)
- [x] DID resolution and public key extraction
- [x] Direct file sharing between DIDs
- [x] Cross-PDS shared file download (via --grant flag)
- [x] Grant listing (shared command)
- [x] AppView indexer (grants + keyrings from firehose)
- [x] AppView REST API with DID-scoped Ed25519 auth
- [x] Folder hierarchy (mkdir, tree, path-aware rm/mv/cat/upload)
- [ ] Grant discovery (inbox command — queries AppView)
- [ ] Keyring-based group sharing
- [ ] Web UI (SPA frontend)

## Development

```sh
cargo test           # run all tests
cargo clippy         # lint
cargo fmt            # format
```

CI runs on [Tangled](https://tangled.org) via `.tangled/workflows/test.yml`.

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## Lexicons

Custom AT Protocol schemas live in `lexicons/`. See [lexicons/README.md](lexicons/README.md) for the full schema documentation and [lexicons/EXAMPLES.md](lexicons/EXAMPLES.md) for annotated example records.

## License

[AGPL-3.0](LICENSE)
