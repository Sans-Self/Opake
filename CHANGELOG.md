# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Security
- Fix ContentKey Debug impl to redact secret bytes (#49)
- Add file permission hardening for sensitive config and key files (#8)
- Remove bearer token authentication fallback from AppView (#26)

### Added
- Migrate from chainlink to crosslink and slim project docs (#137)
- Update docs for security hardening and opake-derive crate (#4)
- Add inbox CLI command for discovering shared grants via appview (#7)
- Audit workspace dependencies for consolidation and upgrades (#25)
- Add AppView production readiness: clap, DID auth, XDG, health, docs (#34)
- Update docs to reflect module directory restructuring (#40)
- Add verbose flags for CLI debug output (#43)
- Add keyring rotation history to preserve member access to pre-rotation documents (#48)
- Add cross-PDS download for keyring members (#51)
- Add keyring-based group sharing (#119)
- Add keyring-based group sharing for multi-user access control (#60)
- Add shared command to list outgoing grants (#121)
- Add MermaidJS flow diagrams and restructure documentation (#63)
- Improve naming consistency and split documents/download.rs (#64)
- Black-box test the full sharing workflow across accounts (#67)
- Add grant-based cross-PDS download for shared files (#66)
- Auto-publish encryption public key on login (#69)
- Add resolve command for DID resolution and public key discovery (#124)
- Add share command to grant document access to another DID (#123)
- Add revoke command to delete grant records (#122)
- Add account management commands and --as flag (#75)
- Improve README with pronunciation guide and formatting polish (#87)
- Add automatic token refresh using refresh_jwt on expired sessions (#101)
- Add filename resolution for rm command (#93)
- Add filename resolution for download command (#94)
- Add MockTransport test infrastructure with FIFO response queue
- Add download command tests with full crypto roundtrip verification
- Update login command to read password from stdin (#112)

### Fixed
- Fix bugs found during black-box integration testing of sharing workflow (#70)
- Fix base64 padding mismatch when decoding PDS $bytes fields (#68)
- Fix missing HTTP status checks in XRPC client (#104)

### Changed
- Update docs to reflect keyring rotation history (#46)
- Add keyring member management (add-member, remove-member) (#56)
- Add CLI keyring commands and local group key store (#57)
- Add keyrings core module with create and list operations (#58)
- Implement symmetric key wrapping primitives in crypto.rs (#59)
- Add automatic public key publishing on login (#77)
- Replace raw [u8; 32] with X25519PublicKey/X25519PrivateKey type aliases (#72)
- Split client.rs into module directory for transport, xrpc, and DID resolution (#71)
- Remove --permissions flag from share command (#73)
- Add per-account session and identity persistence (#84)
- Add multi-account config struct and per-account storage layout (#85)
- Add publicKey lexicon and PublicKeyRecord struct (#80)
- Add document deletion via com.atproto.repo.deleteRecord (#127)
- Add document listing via com.atproto.repo.listRecords (#128)
- Extract AT Protocol primitives into dedicated atproto module
- Consolidate XRPC response checking into send_checked method
- Add file download with client-side decryption (#129)
- Update outdated dependencies (reqwest 0.13, toml) (#106)
- Test upload command against real PDS (#107)
- Add file upload with client-side encryption (#130)
- Add local keystore for session and key persistence (#126)
- Add asymmetric key wrapping (ECDH-ES+A256KW) (#131)
- Add AES-256-GCM content encryption and decryption (#132)
- Fix WASM compilation for opake-core by enabling getrandom js feature (#108)
- Add PDS authentication via com.atproto.server.createSession (#133)
