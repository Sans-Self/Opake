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

# download and decrypt
opake download photo.jpg
opake download photo.jpg -o ~/Downloads/copy.jpg

# delete
opake rm photo.jpg

# remove an account
opake logout --did did:plc:bob456
```

Commands accept either a filename or an `at://` URI. If a filename matches multiple documents, you'll be prompted to use the full URI. The `--as` flag accepts a handle or DID and works with any command.

## Project Structure

```
crates/
  opake-core/    # encryption, records, XRPC client (WASM-compatible)
  opake-cli/     # CLI binary wrapping opake-core
lexicons/        # AT Protocol lexicon schemas (app.opake.cloud.*)
```

`opake-core` is platform-agnostic and compiles to WASM — it will power both the CLI and a future web UI.

## Encryption Model

Every file gets a random AES-256-GCM content key. That key is wrapped (asymmetrically encrypted) to authorized DIDs using x25519-hkdf-a256kw. Two sharing modes:

- **Direct encryption** — content key wrapped individually to each recipient's DID public key
- **Keyring encryption** — a named group shares a group key; documents are wrapped under the group key; adding a member to the keyring grants access to all its documents

Revoking access means deleting the grant record. True forward secrecy requires re-encrypting the blob with a new content key (supported by the schema, not enforced).

## Roadmap

- [x] CLI foundation (auth, upload, download, ls, rm)
- [x] Client-side AES-256-GCM encryption
- [x] Asymmetric key wrapping (x25519-hkdf-a256kw)
- [x] Automatic token refresh
- [x] Multi-account support (--as flag, logout, set-default, accounts)
- [x] Public key discovery (app.opake.cloud.publicKey record)
- [ ] DID resolution and public key extraction
- [ ] Direct file sharing between DIDs
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
