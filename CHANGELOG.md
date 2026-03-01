# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- Add automatic token refresh using refresh_jwt on expired sessions (#34)
- Add filename resolution for rm command (#42)
- Add filename resolution for download command (#41)
- Add MockTransport test infrastructure with FIFO response queue
- Add download command tests with full crypto roundtrip verification
- Update login command to read password from stdin (#23)

### Fixed
- Fix missing HTTP status checks in XRPC client (#31)

### Changed
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
