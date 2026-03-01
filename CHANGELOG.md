# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- Add cross-PDS download for keyring members (#84)
- Add keyring-based group sharing (#16)
- Add keyring-based group sharing for multi-user access control (#75)
- Add shared command to list outgoing grants (#14)
- Add MermaidJS flow diagrams and restructure documentation (#72)
- Improve naming consistency and split documents/download.rs (#71)
- Black-box test the full sharing workflow across accounts (#68)
- Add grant-based cross-PDS download for shared files (#69)
- Auto-publish encryption public key on login (#66)
- Add resolve command for DID resolution and public key discovery (#11)
- Add share command to grant document access to another DID (#12)
- Add revoke command to delete grant records (#13)
- Add account management commands and --as flag (#60)
- Improve README with pronunciation guide and formatting polish (#48)
- Add automatic token refresh using refresh_jwt on expired sessions (#34)
- Add filename resolution for rm command (#42)
- Add filename resolution for download command (#41)
- Add MockTransport test infrastructure with FIFO response queue
- Add download command tests with full crypto roundtrip verification
- Update login command to read password from stdin (#23)

### Fixed
- Fix bugs found during black-box integration testing of sharing workflow (#65)
- Fix base64 padding mismatch when decoding PDS $bytes fields (#67)
- Fix missing HTTP status checks in XRPC client (#31)

### Changed
- Add keyring member management (add-member, remove-member) (#79)
- Add CLI keyring commands and local group key store (#78)
- Add keyrings core module with create and list operations (#77)
- Implement symmetric key wrapping primitives in crypto.rs (#76)
- Add automatic public key publishing on login (#58)
- Replace raw [u8; 32] with X25519PublicKey/X25519PrivateKey type aliases (#63)
- Split client.rs into module directory for transport, xrpc, and DID resolution (#64)
- Remove --permissions flag from share command (#62)
- Add per-account session and identity persistence (#51)
- Add multi-account config struct and per-account storage layout (#50)
- Add publicKey lexicon and PublicKeyRecord struct (#55)
- Add document deletion via com.atproto.repo.deleteRecord (#8)
- Add document listing via com.atproto.repo.listRecords (#7)
- Extract AT Protocol primitives into dedicated atproto module
- Consolidate XRPC response checking into send_checked method
- Add file download with client-side decryption (#6)
- Update outdated dependencies (reqwest 0.13, toml) (#29)
- Test upload command against real PDS (#28)
- Add file upload with client-side encryption (#5)
- Add local keystore for session and key persistence (#9)
- Add asymmetric key wrapping (ECDH-ES+A256KW) (#4)
- Add AES-256-GCM content encryption and decryption (#3)
- Fix WASM compilation for opake-core by enabling getrandom js feature (#27)
- Add PDS authentication via com.atproto.server.createSession (#2)
