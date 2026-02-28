# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- Update login command to read password from stdin (#23)

### Fixed

### Changed
- Update outdated dependencies (reqwest 0.13, toml) (#29)
- Test upload command against real PDS (#28)
- Add file upload with client-side encryption (#5)
- Add local keystore for session and key persistence (#9)
- Add asymmetric key wrapping (ECDH-ES+A256KW) (#4)
- Add AES-256-GCM content encryption and decryption (#3)
- Fix WASM compilation for opake-core by enabling getrandom js feature (#27)
- Add PDS authentication via com.atproto.server.createSession (#2)
