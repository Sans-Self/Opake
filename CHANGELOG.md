# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Security
- Fix ContentKey Debug impl to redact secret bytes [#49](https://issues.opake.app/issues/49.html)
- Add file permission hardening for sensitive config and key files [#8](https://issues.opake.app/issues/8.html)
- Remove bearer token authentication fallback from AppView [#26](https://issues.opake.app/issues/26.html)

### Added
- Add device-to-device key pairing via PDS [#183](https://issues.opake.app/issues/183.html)
- Wire authenticated API layer for PDS and AppView [#174](https://issues.opake.app/issues/174.html)
- Rewrite web auth store as discriminated union state machine [#172](https://issues.opake.app/issues/172.html)
- Add OAuth client infrastructure to web frontend [#171](https://issues.opake.app/issues/171.html)
- Refactor cabinet components to use daisyUI semantic classes [#166](https://issues.opake.app/issues/166.html)
- Add web identity resolution [#152](https://issues.opake.app/issues/152.html)
- Add web login and account switching [#144](https://issues.opake.app/issues/144.html)
- Cache URI → name mappings for DirectoryTree resolution [#155](https://issues.opake.app/issues/155.html)
- Add tree command to display document hierarchy [#90](https://issues.opake.app/issues/90.html)
- Add handle-based login with automatic PDS resolution [#182](https://issues.opake.app/issues/182.html)
- Implement web login flow with AT Protocol OAuth [#167](https://issues.opake.app/issues/167.html)
- Add AT Protocol OAuth (DPoP) for CLI authentication [#175](https://issues.opake.app/issues/175.html)
- Wire opake-core WASM into web frontend [#163](https://issues.opake.app/issues/163.html)
- Add cat command to read and display file contents [#154](https://issues.opake.app/issues/154.html)
- Add directory record type and mkdir command [#98](https://issues.opake.app/issues/98.html)
- Add issue tracker link to README [#142](https://issues.opake.app/issues/142.html)
- Add crosslink-issue-renderer submodule and CI/CD pipeline [#141](https://issues.opake.app/issues/141.html)
- Migrate from chainlink to crosslink and slim project docs [#137](https://issues.opake.app/issues/137.html)
- Update docs for security hardening and opake-derive crate [#4](https://issues.opake.app/issues/4.html)
- Add inbox CLI command for discovering shared grants via appview [#7](https://issues.opake.app/issues/7.html)
- Audit workspace dependencies for consolidation and upgrades [#25](https://issues.opake.app/issues/25.html)
- Add AppView production readiness: clap, DID auth, XDG, health, docs [#34](https://issues.opake.app/issues/34.html)
- Update docs to reflect module directory restructuring [#40](https://issues.opake.app/issues/40.html)
- Add verbose flags for CLI debug output [#43](https://issues.opake.app/issues/43.html)
- Add keyring rotation history to preserve member access to pre-rotation documents [#48](https://issues.opake.app/issues/48.html)
- Add cross-PDS download for keyring members [#51](https://issues.opake.app/issues/51.html)
- Add keyring-based group sharing [#119](https://issues.opake.app/issues/119.html)
- Add keyring-based group sharing for multi-user access control [#60](https://issues.opake.app/issues/60.html)
- Add shared command to list outgoing grants [#121](https://issues.opake.app/issues/121.html)
- Add MermaidJS flow diagrams and restructure documentation [#63](https://issues.opake.app/issues/63.html)
- Improve naming consistency and split documents/download.rs [#64](https://issues.opake.app/issues/64.html)
- Black-box test the full sharing workflow across accounts [#67](https://issues.opake.app/issues/67.html)
- Add grant-based cross-PDS download for shared files [#66](https://issues.opake.app/issues/66.html)
- Auto-publish encryption public key on login [#69](https://issues.opake.app/issues/69.html)
- Add resolve command for DID resolution and public key discovery [#124](https://issues.opake.app/issues/124.html)
- Add share command to grant document access to another DID [#123](https://issues.opake.app/issues/123.html)
- Add revoke command to delete grant records [#122](https://issues.opake.app/issues/122.html)
- Add account management commands and --as flag [#75](https://issues.opake.app/issues/75.html)
- Improve README with pronunciation guide and formatting polish [#87](https://issues.opake.app/issues/87.html)
- Add automatic token refresh using refresh_jwt on expired sessions [#101](https://issues.opake.app/issues/101.html)
- Add filename resolution for rm command [#93](https://issues.opake.app/issues/93.html)
- Add filename resolution for download command [#94](https://issues.opake.app/issues/94.html)
- Add MockTransport test infrastructure with FIFO response queue
- Add download command tests with full crypto roundtrip verification
- Update login command to read password from stdin [#112](https://issues.opake.app/issues/112.html)

### Fixed
- Fix bugs found during black-box integration testing of sharing workflow [#70](https://issues.opake.app/issues/70.html)
- Fix base64 padding mismatch when decoding PDS $bytes fields [#68](https://issues.opake.app/issues/68.html)
- Fix missing HTTP status checks in XRPC client [#104](https://issues.opake.app/issues/104.html)

### Changed
- Add browser key storage with IndexedDB and Web Crypto API [#160](https://issues.opake.app/issues/160.html)
- Add inbox command for grant discovery via AppView [#162](https://issues.opake.app/issues/162.html)
- Port Figma Make cabinet design into web frontend [#165](https://issues.opake.app/issues/165.html)
- Amend web scaffold into WASM commit [#164](https://issues.opake.app/issues/164.html)
- Update blackbox tests and docs for new commands [#159](https://issues.opake.app/issues/159.html)
- Add path-aware mv command [#157](https://issues.opake.app/issues/157.html)
- Add path-aware upload with directory placement [#158](https://issues.opake.app/issues/158.html)
- Add path-aware rm with recursive directory deletion [#156](https://issues.opake.app/issues/156.html)
- Update docs to reflect keyring rotation history [#46](https://issues.opake.app/issues/46.html)
- Add keyring member management (add-member, remove-member) [#56](https://issues.opake.app/issues/56.html)
- Add CLI keyring commands and local group key store [#57](https://issues.opake.app/issues/57.html)
- Add keyrings core module with create and list operations [#58](https://issues.opake.app/issues/58.html)
- Implement symmetric key wrapping primitives in crypto.rs [#59](https://issues.opake.app/issues/59.html)
- Add automatic public key publishing on login [#77](https://issues.opake.app/issues/77.html)
- Replace raw [u8; 32] with X25519PublicKey/X25519PrivateKey type aliases [#72](https://issues.opake.app/issues/72.html)
- Split client.rs into module directory for transport, xrpc, and DID resolution [#71](https://issues.opake.app/issues/71.html)
- Remove --permissions flag from share command [#73](https://issues.opake.app/issues/73.html)
- Add per-account session and identity persistence [#84](https://issues.opake.app/issues/84.html)
- Add multi-account config struct and per-account storage layout [#85](https://issues.opake.app/issues/85.html)
- Add publicKey lexicon and PublicKeyRecord struct [#80](https://issues.opake.app/issues/80.html)
- Add document deletion via com.atproto.repo.deleteRecord [#127](https://issues.opake.app/issues/127.html)
- Add document listing via com.atproto.repo.listRecords [#128](https://issues.opake.app/issues/128.html)
- Extract AT Protocol primitives into dedicated atproto module
- Consolidate XRPC response checking into send_checked method
- Add file download with client-side decryption [#129](https://issues.opake.app/issues/129.html)
- Update outdated dependencies (reqwest 0.13, toml) [#106](https://issues.opake.app/issues/106.html)
- Test upload command against real PDS [#107](https://issues.opake.app/issues/107.html)
- Add file upload with client-side encryption [#130](https://issues.opake.app/issues/130.html)
- Add local keystore for session and key persistence [#126](https://issues.opake.app/issues/126.html)
- Add asymmetric key wrapping (ECDH-ES+A256KW) [#131](https://issues.opake.app/issues/131.html)
- Add AES-256-GCM content encryption and decryption [#132](https://issues.opake.app/issues/132.html)
- Fix WASM compilation for opake-core by enabling getrandom js feature [#108](https://issues.opake.app/issues/108.html)
- Add PDS authentication via com.atproto.server.createSession [#133](https://issues.opake.app/issues/133.html)
