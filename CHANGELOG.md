# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- Update login command to read password from stdin (#23)

### Fixed

### Changed
- Add AES-256-GCM content encryption and decryption (#3)
- Fix WASM compilation for opake-core by enabling getrandom js feature (#27)
- Add PDS authentication via com.atproto.server.createSession (#2)
