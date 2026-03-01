# Opake

**/oʊˈpɑːk/** — like "opaque," but Dutch-flavored.

An encrypted personal cloud built on the [AT Protocol](https://atproto.com).

Opake uses your existing PDS as a storage and identity layer. Files are encrypted client-side with AES-256-GCM before upload — the PDS only ever sees ciphertext. Custom lexicons under `app.opake.cloud.*` give structure to documents, encryption metadata, and sharing grants.

Your data is opaque to everyone without the key. That's the point.

## How It Works

```
plaintext file
  → encrypt with random AES-256-GCM key
  → upload ciphertext blob to PDS
  → wrap content key to owner's DID public key
  → store metadata as app.opake.cloud.document record
```

No middleware, no AppView, no modifications to the PDS. All crypto happens on your machine.

## Build From Source

Requires Rust 1.75+.

```sh
git clone <repo-url>
cd opake.dev
cargo build --release
```

The binary lands at `target/release/opake`.

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

# download a shared file from another user (via grant URI)
opake download --grant at://did:plc:abc/app.opake.cloud.grant/tid123

# delete
opake rm photo.jpg

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

Commands accept either a filename or an `at://` URI. If a filename matches multiple documents, you'll be prompted to use the full URI.

The `--as` flag works with document commands (`upload`, `download`, `ls`, `rm`, `share`, `shared`, `revoke`) and accepts a handle or DID.

## Architecture

Two crates: `opake-core` (platform-agnostic library, compiles to WASM) and `opake-cli` (thin CLI wrapper). See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the encryption model, crate structure, and design decisions. See [docs/FLOWS.md](docs/FLOWS.md) for sequence diagrams of every operation.

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
- [ ] Grant discovery (inbox command)
- [ ] Keyring-based group sharing
- [ ] Folder hierarchy
- [ ] Web UI (Rust/Axum AppView + SPA)

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
